# ADR-0002: Workspace layout and crate boundaries

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A multi-crate Cargo workspace lets us keep compile times down, enforce
dependency direction, and publish subsets of the codebase if downstream
users want them. It also lets us draw firm lines between the engine
(no I/O), the binary (the only place CLI parsing lives), and the
integration shims (one crate per host family). A flat single-crate
layout would force every consumer to compile every host adapter.

We need a layout that:

- Keeps the engine library (`kirkstratum-core`) free of all I/O on the hot
  path so it can be embedded in another binary or reused as a Rust
  library without dragging in axum, tokio, or any host SDK.
- Lets the cross-host adapter code (ADR-0008) live in one crate so a
  drift test can cover it without crossing crate boundaries.
- Has exactly one binary crate, owning clap-derive CLI parsing, config
  loading, and the entry point that wires the engine to the host
  adapter.
- Has at least one `cdylib` *only if* we later expose the engine to
  Python or Node. We do not commit to this in ADR-0001; the workspace
  layout must accommodate it without forcing it.

## Decision

The workspace is a Cargo workspace rooted at `Cargo.toml` with member
crates laid out as follows:

```
KirkForge-Plugin2/
├── Cargo.toml                        # workspace manifest
├── rust-toolchain.toml               # pinned toolchain
├── crates/
│   ├── kirkstratum-core/                 # the engine (library)
│   ├── kirkstratum-cli/                  # the binary
│   └── kirkstratum-hosts/                # cross-host adapters + drift test
├── plugins/
│   └── stratum-agent-hooks/          # marketplace plugin glue
│       ├── .claude-plugin/plugin.json
│       ├── hooks/hooks.json
│       └── README.md
├── docs/
│   ├── adr/                          # this directory
│   └── rules/                        # canonical ruleset (ADR-0008)
├── examples/
│   └── fixtures/                     # JSON test fixtures for ADR-0017
└── README.md
```

### `kirkstratum-core`

The engine. Library crate, no `[[bin]]`. Dependencies are limited to:
`serde`, `serde_json`, `thiserror`, `tracing`, `rayon`, `blake3`,
`sha2`, `toml`, `memchr`, `parking_lot`, and (gated) `rusqlite`.

It contains:

- `src/pipeline/` — the `ReformatTransform` and `OffloadTransform`
  traits, the `CompressionPipeline` orchestrator (ADR-0003, ADR-0005).
- `src/store/` — the `OffloadStore` trait and `InMemory`/`Sqlite`/
  `File` backends (ADR-0004).
- `src/content/` — `ContentType`, the `detect()` chain (ADR-0014).
- `src/mode.rs` — the `Mode` enum and config (ADR-0006).
- `src/error.rs` — the three-variant `TransformError` (ADR-0011).
- `src/config.rs` — `PipelineConfig` + the `include_str!`-embedded
  TOML default (ADR-0007).

It must not import anything that touches the network, the filesystem
outside `std::env::temp_dir()`, or a host SDK. The `Store` backends
are an exception: `File` and `Sqlite` are allowed to touch disk
because that is their purpose, but they live behind the trait so the
core can be used with only `InMemory`.

### `kirkstratum-cli`

The binary. Single `[[bin]]` named `stratum`. Clap-derive parsing
lives here and only here. This crate depends on `kirkstratum-core` and
`kirkstratum-hosts` and owns:

- `src/main.rs` — entry point, top-level dispatch.
- `src/args.rs` — `CliArgs`, `Commands` enum (subcommands are
  `run`, `init`, `rules`, `mode`, `version`).
- `src/config_loader.rs` — env > CLI > file > default precedence
  (ADR-0007, ADR-0016).
- `src/init.rs` — `stratum init hook ensure` (ADR-0010).

The CLI crate may depend on `tokio` for graceful shutdown and on
`clap`, `anyhow` (for the binary only — the core uses `thiserror`).
The core must never gain a `tokio` dependency.

### `kirkstratum-hosts`

Cross-host adapters and the drift test. Library crate, no binary.
Owns:

- `src/lib.rs` — the `Host` enum and `emit_to(host, event, payload)`
  shim (ADR-0009).
- `src/adapters/` — one submodule per supported host family. Each
  adapter is a thin wrapper that calls `emit_to` and provides the
  canonical instruction builder (ADR-0008).
- `tests/copy_drift.rs` — the invariant test that fails CI when any
  per-host instruction copy drifts from the canonical ruleset
  (ADR-0008, ADR-0017).

This crate may depend on `serde_json` and `kirkstratum-core`. It must not
depend on any host's proprietary SDK; the adapters are pure
data-shuffling code.

### `plugins/stratum-agent-hooks/`

The marketplace plugin glue. Not a Rust crate; a directory with a
manifest. The marketplace manifest at the repo root
(`.claude-plugin/marketplace.json`) points at this directory.

The `hooks/hooks.json` declares `SessionStart`, `PreToolUse`, and
`UserPromptSubmit` events that all invoke the `stratum` binary
(`stratum init hook ensure`, `stratum rules emit`, `stratum mode
track`). The shell command does the work; the plugin manifest is just
metadata.

### Workspace manifest

The workspace `Cargo.toml` declares:

```toml
[workspace]
resolver = "2"
members = [
    "crates/kirkstratum-core",
    "crates/kirkstratum-cli",
    "crates/kirkstratum-hosts",
]

[workspace.package]
edition = "2021"
rust-version = "1.75"     # MSRV; bump via ADR only
license = "MIT OR Apache-2.0"
repository = "https://example.invalid/stratum"   # placeholder

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
rayon = "1"
blake3 = "1"
sha2 = "0.10"
toml = "0.8"
memchr = "2"
parking_lot = "0.12"
clap = { version = "4", features = ["derive", "env"] }
anyhow = "1"

[profile.release]
strip = "symbols"
lto = "thin"
codegen-units = 1

[profile.ci]
inherits = "release"
debug = "line-tables-only"
incremental = false
```

## Consequences

Negative first:

- Three crates is one more than the minimum. A reviewer who only
  wants to ship a binary will ask why we need `kirkstratum-hosts` as a
  separate crate; the answer (the drift test needs a single crate to
  own every adapter) must be defended each time.
- Publishing `kirkstratum-core` to crates.io requires a separate
  `Cargo.toml` discipline. Forgetting to keep `[workspace.package]`
  in sync with each member's `[package]` is a classic footgun.
- `kirkstratum-core` cannot use `tokio`, which rules out async-by-default
  transforms. If a future transform genuinely needs async I/O (e.g.
  network offload), it must negotiate an exception through ADR
  revision rather than sneak `tokio` into core.

Positive:

- The engine can be vendored into another binary (a downstream
  proxy, an MCP server, a test harness) by depending on
  `kirkstratum-core` alone.
- The drift test lives in `kirkstratum-hosts` and is the only place
  outside that crate's `src/` that may read every adapter file. The
  test's blast radius is bounded.
- The CLI binary can be rebuilt to omit the hosts crate entirely
  (`--no-default-features --features=core-only`) for users who want
  a pure library + CLI without the host adapter code, though this
  feature gate is not exposed in the public CLI.
- The `plugins/stratum-agent-hooks/` directory can be replaced or
  removed without touching the Rust workspace. The hook manifest is
  the contract; the Rust binary is the implementation.

## Implementation notes

When scaffolding the workspace:

1. Create the directory tree above before running `cargo new` in each
   crate, so the `members` array matches reality.
2. Pin the toolchain in `rust-toolchain.toml`:
   ```toml
   [toolchain]
   channel = "1.75.0"
   components = ["rustfmt", "clippy", "rust-src"]
   ```
3. The `kirkstratum-cli` `Cargo.toml` has a `[features]` table with
   `default = ["hosts"]` and `hosts = ["dep:kirkstratum-hosts"]` so the
   pure-core build path is one feature flag away.
4. The `kirkstratum-core` `Cargo.toml` has a `[features]` table with
   `default = []` and `sqlite = ["dep:rusqlite"]` so the
   `SqliteOffloadStore` (ADR-0004) is opt-in. The `File` backend
   uses only `std::fs` and is always present.
5. `cargo fmt --all`, `cargo clippy --workspace --all-targets
   -- -D warnings`, and `cargo test --workspace` are the three
   commands that must pass before any commit lands on `main`. The
   CI workflow runs them on Linux, macOS, and Windows.
