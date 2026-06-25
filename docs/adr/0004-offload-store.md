# ADR-0004: OffloadStore trait and backend selection (loud failure)

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

Every offload transform (ADR-0003) needs somewhere to put the bytes it
drops. The store is the *recoverability contract* — without a store,
offload is silent data loss. With the wrong store, offload is a
performance footgun (e.g. an in-memory store in a long-running agent
loop will OOM).

Three backends are plausible:

1. **In-memory** — fast, no persistence, no setup. Good for tests and
   short-lived CLI runs. Catastrophic for long sessions.
2. **SQLite** — durable, single file, WAL-mode concurrency. Good
   default for a developer laptop.
3. **File-based (one file per key)** — simplest possible durability.
   Good for read-only or append-only workloads.

The trait must accommodate all three without leaking backend-specific
quirks. The factory that selects a backend at startup must surface
*every* init error loudly — silent fallback to in-memory when SQLite
fails to open is a documented footgun in this problem space.

## Decision

The trait lives at `crates/stratum-core/src/store/mod.rs`:

```rust
pub trait OffloadStore: Send + Sync {
    /// Store `payload` and return the key under which it was stored.
    /// The trait owns key generation; backends may use content hashes,
    /// random UUIDs, or monotonic counters.
    fn put(&self, payload: &str) -> String;

    /// Retrieve the original payload by key, if present and not expired.
    /// Implementations may perform lazy expiry here (ADR-0004 § Expiry).
    fn get(&self, key: &str) -> Option<String>;

    /// Total number of live entries. Used by tests and by the
    /// `stratum store stats` subcommand.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool { self.len() == 0 }

    /// Name of the backend, for diagnostics and the drift test.
    fn backend_name(&self) -> &'static str;
}
```

### Key generation

All backends use the same key derivation: the first 24 hex characters
of a BLAKE3 hash of the payload. The trait method `put` returns the
key it generated; the caller does not supply one. This makes
duplicate-detection trivial (two transforms that offload the same
bytes share a key, no double-storage) and makes the marker
self-describing (the marker embeds the content hash, so cache
verification is one BLAKE3 call away).

```rust
// In OffloadStore impl, not exposed publicly:
fn derive_key(payload: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(payload.as_bytes());
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    let mut out = String::with_capacity(24);
    for b in &bytes[..12] {
        use std::fmt::Write;
        let _ = write!(out, "{:02x}", b);
    }
    out
}
```

### Backends

```rust
// crates/stratum-core/src/store/mod.rs

pub enum OffloadBackendConfig {
    Memory,
    Sqlite { path: std::path::PathBuf, ttl_seconds: u64 },
    File { dir: std::path::PathBuf, ttl_seconds: u64 },
}

impl OffloadBackendConfig {
    pub fn from_env_and_cli(
        env: &dyn EnvSource,
        cli: &CliStoreArgs,
    ) -> Result<Self, StoreConfigError> { /* ... */ }

    pub fn build(self) -> Result<Box<dyn OffloadStore>, StoreInitError> {
        match self {
            Self::Memory => Ok(Box::new(InMemoryOffloadStore::new())),
            Self::Sqlite { path, ttl_seconds } => {
                SqliteOffloadStore::open(&path, ttl_seconds)
                    .map(|s| Box::new(s) as Box<dyn OffloadStore>)
                    .map_err(StoreInitError::Sqlite)
            }
            Self::File { dir, ttl_seconds } => {
                std::fs::create_dir_all(&dir).map_err(StoreInitError::Io)?;
                Ok(Box::new(FileOffloadStore::new(dir, ttl_seconds)))
            }
        }
    }
}
```

The factory `build()` returns `Err(StoreInitError)` on **any** failure.
There is no `.unwrap_or_default()`, no fallback chain, no log-and-continue.
The CLI binary catches the error and exits with code `78` (`EX_CONFIG`
from `sysexits.h`) and a one-line message naming the backend and the
underlying cause.

This is the loud-failure contract: a misconfigured store is a
deployment error, not a runtime surprise.

### In-memory backend

`InMemoryOffloadStore` is a `parking_lot::RwLock<HashMap<String, String>>`.
It is the test backend and the fallback for `--store=memory` on the
CLI. It exposes a `clear()` method that no other backend has, used
by tests.

### SQLite backend

`SqliteOffloadStore` (gated behind `#[cfg(feature = "sqlite")]` in
`store/sqlite.rs`) opens a single connection in WAL mode:

```sql
CREATE TABLE IF NOT EXISTS offload_entries (
    hash         TEXT PRIMARY KEY,
    original     BLOB NOT NULL,
    created_at   INTEGER NOT NULL,
    ttl_seconds  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS offload_created_at_idx
    ON offload_entries(created_at);
```

The connection is wrapped in a `parking_lot::Mutex<Connection>` (the
existing `r2d2_sqlite` pool is overkill for the workload — one
writer at a time per CLI invocation, occasional reads). On `get`,
the backend checks `created_at + ttl_seconds < now` and returns
`None` if expired (lazy TTL, no background reaper).

The `sqlite` feature is opt-in in `stratum-core/Cargo.toml`:

```toml
[features]
default = []
sqlite = ["dep:rusqlite"]
```

The CLI binary enables it by default; library consumers opt in.

### File backend

`FileOffloadStore` (always present, no feature gate) writes one file
per key under `dir/{shard}/{key}` where `shard = &key[..2]`. Two-level
sharding prevents any single directory from exceeding a few thousand
entries. Read is `fs::read_to_string`. TTL is enforced on read by
checking `mtime + ttl_seconds < now`. No background reaper.

The file backend is the right choice for read-mostly workloads (e.g.
a CI cache) and for environments where SQLite is unavailable.

### Expiry

All backends implement **lazy TTL**: expiry is checked on `get`, not
on a background timer. There is no `purge()` method on the trait;
backends may expose one as an inherent method (e.g. for the
`stratum store purge` CLI subcommand), but the trait contract is
read-side only.

Rationale: a background timer requires a thread, a channel, and
shutdown coordination — three things that have to be right. Lazy
expiry is correct by construction: the only state ever read is
checked before it is returned.

### Stats and observability

Each backend implements `backend_name()`. A future `stratum store
stats` subcommand will call `len()` and `backend_name()`. The
subcommand is not in ADR-0001's MVP and is explicitly deferred.

## Consequences

Negative first:

- Lazy TTL means a key may sit in the store long after it has
  expired, until the next `get` or a manual purge. For workloads
  with many expired entries and few reads, the file/SQLite files
  will grow. The `stratum store purge` subcommand (deferred) is the
  answer.
- The factory returns `Err` on any backend failure, including
  permission errors on `create_dir_all`. A user who runs the CLI in
  a directory they cannot write to gets a hard exit, not a graceful
  fallback. This is deliberate (loud failure) but documented in the
  README's troubleshooting section.
- `parking_lot::Mutex<Connection>` for SQLite is single-threaded by
  construction. A high-throughput proxy that wants concurrent writes
  must swap to a `r2d2_sqlite` pool and rewrite the impl. The pool
  is not the default because the workload is dominated by short,
  rare puts and gets.

Positive:

- The trait owns key generation, so a transform author cannot
  collide keys between backends. The marker is portable across
  backends: `<<stratum:offload:abc123>>` means the same thing whether
  the store is in-memory, SQLite, or file.
- Loud failure on backend init means a misconfigured deployment is
  caught at startup, not at the first offload that silently loses
  bytes.
- The three backends cover the three realistic deployment scenarios
  (test, laptop, server) without forcing a fourth. The "Redis
  backend" temptation is explicitly deferred — if a future use case
  demands it, ADR revision adds a `Redis` variant and a
  `redis = ["dep:redis"]` feature gate.

## Implementation notes

The `OffloadStore` trait lives in `crates/stratum-core/src/store/mod.rs`.
The backends live in submodules:

```
crates/stratum-core/src/store/
├── mod.rs               # trait + config + factory
├── memory.rs            # InMemoryOffloadStore
├── sqlite.rs            # SqliteOffloadStore (feature-gated)
├── file.rs              # FileOffloadStore
└── key.rs               # derive_key(), key validation
```

The factory signature is:

```rust
pub fn build_store(cfg: OffloadBackendConfig) -> Result<Box<dyn OffloadStore>, StoreInitError>
```

The CLI binary wraps this:

```rust
match OffloadBackendConfig::from_env_and_cli(&env, &cli_args)?.build() {
    Ok(store) => { /* proceed */ }
    Err(StoreInitError::Sqlite(e)) => {
        eprintln!("stratum: failed to open sqlite store: {}", e);
        std::process::exit(78);
    }
    Err(StoreInitError::Io(e)) => {
        eprintln!("stratum: store I/O error: {}", e);
        std::process::exit(78);
    }
    Err(StoreInitError::Path(p)) => {
        eprintln!("stratum: invalid store path '{}'", p.display());
        std::process::exit(78);
    }
}
```

Exit code 78 is `EX_CONFIG` from BSD sysexits, the conventional code
for "configuration error". The CLI binary's `exit_code` module
documents the full mapping (see ADR-0016).

The `key.rs` module's `validate_key(s: &str) -> bool` function is
exposed for tests and for the file backend's directory-shard logic:

```rust
pub fn validate_key(s: &str) -> bool {
    s.len() == 24 && s.chars().all(|c| c.is_ascii_hexdigit())
}
```

The marker parser in `pipeline/marker.rs` calls `validate_key` on
the extracted key and returns `None` for malformed markers, which
the orchestrator treats as a `Skipped` (ADR-0011).
