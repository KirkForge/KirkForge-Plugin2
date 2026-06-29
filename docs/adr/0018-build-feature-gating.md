# ADR-0018: Build profile and feature gating discipline

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A plugin binary that ships at 30 MB when it could ship at 5 MB
is not "lazy" — it is wasteful. A binary that takes 8 seconds to
compile from scratch in CI is a tax on every contributor. A
test suite that produces different output in debug vs release is
a maintenance burden.

The build configuration must be:

- Lean: release binary stripped, LTO enabled, single codegen
  unit. Size target: <8 MB.
- Fast in CI: debug build uses line-tables-only debug info,
  no LTO, no strip. CI time target: <3 minutes for `cargo
  test --workspace`.
- Reproducible: pinned toolchain, locked dependencies,
  deterministic feature flags. The same commit produces the
  same binary byte-for-byte.

The feature gates must be:

- Minimal: every feature is opt-in except those required for
  the MVP.
- Documented: the README has a feature matrix; each feature
  has a one-line description.
- Audited: a CI check lists the active features for the
  current build and fails if a feature flag is set without
  justification in the diff.

## Decision

### Workspace Cargo.toml

```toml
# Cargo.toml (workspace root)

[workspace]
resolver = "2"
members = [
    "crates/kirkstratum-core",
    "crates/kirkstratum-cli",
    "crates/kirkstratum-hosts",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT OR Apache-2.0"
repository = "https://github.com/kirkforge/stratum"
authors = ["KirkForge"]

[workspace.dependencies]
# Internal
kirkstratum-core = { path = "crates/kirkstratum-core", version = "0.1.0" }
kirkstratum-hosts = { path = "crates/kirkstratum-hosts", version = "0.1.0" }

# External
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive", "env"] }
clap_complete = "4"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
parking_lot = "0.12"
rayon = "1"
blake3 = "1"
thiserror = "1"
anyhow = "1"
directories = "5"
walkdir = "2"
tempfile = "3"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }

# Optional
rusqlite = { version = "0.31", features = ["bundled"], optional = true }
serde_yaml = { version = "0.9", optional = true }

# Dev
proptest = "1"
assert_cmd = "2"
predicates = "3"

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = "symbols"
debug = false
panic = "abort"

[profile.dev]
opt-level = 0
debug = true
incremental = true

[profile.ci]
inherits = "dev"
debug = "line-tables-only"
incremental = false
```

### Toolchain pin

`rust-toolchain.toml` at the workspace root pins the toolchain:

```toml
[toolchain]
channel = "1.75.0"
components = ["rustfmt", "clippy", "rust-src"]
profile = "minimal"
```

The pin is the *exact* version, not a semver range. A
contributor who upgrades the toolchain does so deliberately,
in a separate commit, with a CI green-light before the
upgrade merges.

### Feature gates in kirkstratum-core

```toml
# crates/kirkstratum-core/Cargo.toml

[features]
default = []
sqlite = ["dep:rusqlite"]
yaml = ["dep:serde_yaml"]
```

- `default = []` — no optional features enabled by default. The
  CLI binary enables `sqlite` because the default offload store
  is SQLite (ADR-0004).
- `sqlite` — gates the `SqliteOffloadStore` (ADR-0004 § SQLite
  backend). Pulls in `rusqlite` with the `bundled` feature so
  no system SQLite is required.
- `yaml` — gates a YAML config parser alternative to TOML.
  Deferred until a user actually wants it; the README does not
  list `yaml` as a supported feature.

A user who wants no SQLite (e.g. air-gapped with no C
compiler) builds with `--no-default-features --features ""`
and gets a binary with only the `InMemoryOffloadStore` and
`FileOffloadStore` backends. The binary works; the runtime
fails loud on SQLite init (ADR-0004) if the user picks
`store = "sqlite"` in their config.

### Feature gates in kirkstratum-cli

```toml
# crates/kirkstratum-cli/Cargo.toml

[features]
default = ["sqlite"]
sqlite = ["kirkstratum-core/sqlite"]
```

The CLI's default features enable `sqlite` because the default
store is SQLite. A user who wants a leaner binary builds with
`--no-default-features`.

### Feature gates in kirkstratum-hosts

```toml
# crates/kirkstratum-hosts/Cargo.toml

[features]
default = []
```

`kirkstratum-hosts` has no default features. It is a library; the
host adapter author opts in to whatever they need.

### CI matrix

The CI workflow builds the binary under three feature-flag
configurations:

```yaml
# .github/workflows/ci.yml (sketch)

jobs:
  test:
    strategy:
      matrix:
        features:
          - ""                              # default
          - "--no-default-features"          # minimum
          - "--features sqlite"              # explicit
    steps:
      - run: cargo build ${{ matrix.features }}
      - run: cargo test ${{ matrix.features }}
      - run: cargo clippy --all-targets ${{ matrix.features }} -- -D warnings
```

The matrix catches:

- A regression in the default build (the most common case).
- A regression in the minimum build (no SQLite; the
  `InMemoryOffloadStore` and `FileOffloadStore` must work).
- A regression in the explicit-feature build (the case where
  a user enables a specific feature flag).

### Reproducible builds

CI sets:

```yaml
env:
  CARGO_INCREMENTAL: "0"
  CARGO_PROFILE_DEV_DEBUG: "line-tables-only"
  SOURCE_DATE_EPOCH: "0"
  RUSTFLAGS: "--remap-path-prefix $HOME=/build"
```

`SOURCE_DATE_EPOCH=0` makes every `chrono::Utc::now()` call
deterministic. `RUSTFLAGS=--remap-path-prefix` ensures the
binary does not embed the contributor's home directory in
debug info.

The release workflow additionally sets:

```yaml
env:
  CARGO_PROFILE_RELEASE_STRIP: "symbols"
  CARGO_PROFILE_RELEASE_LTO: "thin"
```

The release binary is reproducible on a per-commit basis:
`git rev-parse HEAD` plus the toolchain pin plus the locked
dependencies determine the binary exactly.

### Size budget

The release binary target is <8 MB. CI measures the binary size
and fails if it exceeds the budget:

```bash
SIZE=$(stat -c%s target/release/stratum)
if [ "$SIZE" -gt 8388608 ]; then
    echo "stratum: binary size $SIZE exceeds 8MB budget"
    exit 1
fi
```

The budget is enforced, not aspirational. A PR that adds a
heavy dependency must justify the size delta in the PR
description.

### Compile-time budget

CI measures `cargo build --release` wall time and fails if it
exceeds 5 minutes:

```bash
time cargo build --release
```

A 5-minute compile is the threshold for "this is taking too
long, find a faster path". A 10-minute compile is a contributor
experience failure.

### What this forbids

- Unconditional `tokio` dependency. The MVP is synchronous
  (ADR-0001). A future async story is a separate ADR.
- Unconditional `serde_json::Value` parsing in hot paths.
  Strongly-typed structs are preferred (ADR-0016, ADR-0009).
- `unsafe` code without a `// SAFETY:` comment and a
  test that exercises the unsafe path.
- Build scripts (`build.rs`) that download assets at build
  time. The skill generator (ADR-0008) reads from the
  filesystem, not the network.

### What this allows

- A single `tokio` runtime in a future ADR. Not now.
- Native dependencies via the `bundled` feature (`rusqlite`).
  This is the only native dependency in the MVP.
- `#[cfg(feature = "...")]` gating throughout `kirkstratum-core`
  to keep the minimum binary small.

## Consequences

Negative first:

- Three CI matrix entries is three times the CI time for
  `cargo build`. The total CI time budget is ~9 minutes
  (3 × 3 min); a single-feature build would be faster but
  would miss cross-feature regressions.
- The 8 MB binary budget is tight. Adding `tokio` or a heavy
  crypto crate would blow it. A future contributor who needs
  one of those must negotiate the budget.
- `SOURCE_DATE_EPOCH=0` makes `chrono::Utc::now()` return
  the Unix epoch. Tests that assert on the current time must
  inject a clock; the `chrono::Clock` trait is the standard
  injection point.

Positive:

- The release binary is small, fast, and reproducible.
- The minimum-feature binary works on the most constrained
  target (no C compiler, no system SQLite).
- The feature matrix is documented in the README, audited in
  CI, and visible in the build output (`cargo build
  --features ...` prints the active features).
- The toolchain pin means a contributor's local toolchain
  cannot silently drift from CI's.

## Implementation notes

The `clippy.toml` at the workspace root configures clippy for
the workspace:

```toml
# clippy.toml
disallowed-methods = [
    { path = "std::panic::panic", reason = "use Result and TransformError (ADR-0011)" },
    { path = "std::panic::unwind", reason = "no unwinding across FFI boundaries" },
]
```

The `.cargo/config.toml` at the workspace root configures
build-time aliases:

```toml
# .cargo/config.toml
[alias]
xtask = "run --bin xtask --"
bloat = "bloat --release --crates"
```

The `cargo-bloat` alias is for the contributor who wants to
see what is consuming the binary budget; it is not part of CI.

The README's "Building from source" section documents:

```bash
# Default build (with SQLite)
cargo build --release

# Minimum build (no SQLite)
cargo build --release --no-default-features

# Run tests
cargo test --workspace

# Audit binary size
cargo bloat --release --crates
```

The four commands are the contributor's entry points.

CI's `target/` directory is cached between runs:

```yaml
- uses: Swatinem/rust-cache@v2
  with:
    workspaces: "."
```

The cache is keyed on the toolchain pin and the `Cargo.lock`
hash. A change to either invalidates the cache.

### README feature matrix

The README has a feature matrix:

| Feature   | Default | Adds                          | Use when                            |
|-----------|---------|-------------------------------|-------------------------------------|
| `sqlite`  | yes (CLI only) | `rusqlite` (bundled)    | You want the SQLite offload store. |
| `yaml`    | no      | `serde_yaml`                  | You want YAML config (experimental). |

The matrix is auto-generated by a build script that reads the
`[features]` sections of each crate's `Cargo.toml` and renders
the table. The generator lives at `xtask/src/feature_matrix.rs`.

The auto-generation is the load-bearing trick: the matrix is
never out of sync with the code.