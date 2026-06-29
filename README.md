# Stratum

A Rust workspace that provides a compression and rules pipeline for AI agent
context. It takes large payloads (logs, search results, diffs, JSON), runs a
configurable set of content and output transforms, and emits a compacted context
string suitable for feeding to an LLM.

## Workspace crates

| Crate | Path | Responsibility |
|-------|------|----------------|
| `kirkstratum-core` | `crates/kirkstratum-core` | Content types, pipeline orchestrator, in-memory offload store, mode enum, and embedded TOML config |
| `kirkstratum-hosts` | `crates/kirkstratum-hosts` | Host adapter helpers, including the canonical ruleset filter |
| `kirkstratum-cli` | `crates/kirkstratum-cli` | `stratum` binary and command-line interface |

## Installation

Install the `stratum` CLI from crates.io:

```bash
cargo install stratum
```

Or build from source with the latest `main` branch:

```bash
cargo install --path crates/kirkstratum-cli
```

## Build

```bash
cargo build --workspace
```

The workspace pins Rust 1.88.0 via `rust-toolchain.toml`, which also declares
`rustfmt` and `clippy` components so every environment uses the same toolchain.

## Test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

For a release-equivalent build (used in CI):

```bash
cargo build --workspace --locked --all-features --profile ci
```

## Run

```bash
# Pipe stdin through the default pipeline
echo "hello world" | cargo run --bin stratum -- run

# Run in a different mode (off/lite/full/ultra)
cargo run --bin stratum -- --mode off run < input.txt
STRATUM_MODE=ultra cargo run --bin stratum -- run < input.txt

# Show the active mode
cargo run --bin stratum -- mode

# Emit the canonical ruleset for a mode
cargo run --bin stratum -- rules

# Emit rules for a specific mode (subcommand mode wins over global flag)
cargo run --bin stratum -- rules --mode ultra
cargo run --bin stratum -- --mode off rules --mode ultra

# Validate config and exit with code 78 on errors
cargo run --bin stratum -- config --validate

# Print the merged effective config
cargo run --bin stratum -- config

# Show which config sources contributed (embedded, XDG, config-dir, explicit)
cargo run --bin stratum -- config --sources

# Generate shell completion scripts
cargo run --bin stratum -- completion bash > /tmp/stratum.bash

# Initialise the default config in $XDG_CONFIG_HOME/stratum/
cargo run --bin stratum -- init

# Initialise the default config in a custom directory
mkdir -p ~/.config/stratum-custom
cargo run --bin stratum -- init --config-dir ~/.config/stratum-custom

# Apply the pipeline to a file (auto-detect content type)
cargo run --bin stratum -- apply file.log

# Force a content type and override mode/token budget
cargo run --bin stratum -- apply file.txt \
  --content-type json-object --mode ultra --token-budget 1024

# Preview what the pipeline would do without transforming output
cargo run --bin stratum -- --dry-run --json run < large.log

# Set token budget via env var to tune the bloat heuristic
STRATUM_TOKEN_BUDGET=512 cargo run --bin stratum -- run < large.log
```

## Library usage

`kirkstratum-core` can be used as a library dependency to embed the pipeline in
host adapters or other Rust tools:

```rust
use kirkstratum_core::config::PipelineConfig;
use kirkstratum_core::content::ContentType;
use kirkstratum_core::mode::Mode;
use kirkstratum_core::pipeline::{CompressionContext, CompressionPipeline};
use kirkstratum_core::store::InMemoryOffloadStore;

let mut pipeline = CompressionPipeline::new();
pipeline.register_content_transform(|s| s.trim_end().to_string());

let store = InMemoryOffloadStore::new();
let ctx = CompressionContext::default().with_token_budget(1024);

let out = pipeline.run(
    "hello world\n",
    ContentType::PlainText,
    &ctx,
    &store,
    &PipelineConfig::default(),
    Mode::Full,
);
```

A runnable version of this snippet is in
[`crates/kirkstratum-core/examples/library_usage.rs`](crates/kirkstratum-core/examples/library_usage.rs).

## Features

| Feature | Default | Crate | Description |
|---------|---------|-------|-------------|
| *(default)* | yes | `kirkstratum-core` | All core content types, pipeline orchestrator, and in-memory offload store |
| `sqlite` | no | `kirkstratum-core` | Persistent SQLite-backed offload store (planned; see ADR-0004) |

## Configuration

Default configuration is embedded in `crates/kirkstratum-core/config/pipeline.toml`.
Override values at runtime with `--config <path>` / `STRATUM_CONFIG`, or place a
`pipeline.toml` in a directory supplied with `--config-dir` / `STRATUM_CONFIG_DIR`.
A default override file is also read from `$XDG_CONFIG_HOME/stratum/pipeline.toml`
if it exists.

Precedence (highest to lowest):
1. `STRATUM_CONFIG` / `--config`
2. `--config-dir/pipeline.toml` / `STRATUM_CONFIG_DIR`
3. `$XDG_CONFIG_HOME/stratum/pipeline.toml`
4. Embedded default

## Scriptable output

Subcommands that emit structured output (`config`, `rules`, `mode`, `version`)
accept a global `--json` flag for machine-readable output:

```bash
cargo run --bin stratum -- --json config
cargo run --bin stratum -- --json rules --mode ultra
cargo run --bin stratum -- --json mode
cargo run --bin stratum -- --json version
```

## Input limits

To prevent accidental memory pressure, `run` and `apply` enforce a default
maximum input size of 50 MiB. File inputs are rejected without reading when
metadata shows they exceed the limit; stdin and remaining files are read
incrementally and capped so the process fails fast instead of buffering the
entire payload. Override with `--max-input-size` or the `STRATUM_MAX_INPUT_SIZE`
env var:

```bash
# Allow up to 100 MiB
cargo run --bin stratum -- --max-input-size 104857600 run < large.log
```

## Performance

Benchmarks are in `crates/kirkstratum-core/benches/compression.rs` and run with
`cargo bench -p kirkstratum-core`. The default pipeline is a lightweight
orchestrator: most of the work is content-type detection, bloat-ratio arithmetic,
and optional offload-key generation.

Throughput on the development machine (single core, release build):

| Input | Size | Median throughput |
|-------|------|-------------------|
| small JSON object | ~1 KiB | ~673 MiB/s |
| large log (plain-text detector) | ~1 MiB | ~53 MiB/s |
| large diff | ~512 KiB | ~115 MiB/s |
| worst-case plain text | ~1 MiB | ~122 MiB/s |

> Note: Numbers are from `cargo bench -p kirkstratum-core` on the development
> machine. Throughput scales with input size because fixed overhead is amortized.

## CLI argument validation

Mode and content-type arguments are validated at parse time. Invalid values
print the supported set and exit with code 64 (`EX_USAGE`) instead of being
silently ignored or failing later as internal errors:

```bash
# Rejected at parse time with supported values listed
stratum --mode turbo run
stratum apply --content-type xml file.txt
```

`--token-budget` and `--max-input-size` must be positive integers; `0` is
rejected at parse time.

## Observability

The `stratum` binary initializes a `tracing-subscriber` logger that writes to
stderr. Control log verbosity with `-v`/`-q` or the `RUST_LOG` env var:

```bash
# Increase verbosity (DEBUG)
cargo run --bin stratum -- -v run < input.txt

# Decrease verbosity (WARN)
cargo run --bin stratum -- -q run < input.txt

# Fine-grained filter via RUST_LOG
RUST_LOG=debug cargo run --bin stratum -- run < input.txt
```

Structured spans and events are emitted around subcommand execution and
pipeline runs.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 64 | Usage error (invalid CLI arguments, handled by clap) |
| 65 | Input data error (e.g., oversized input) |
| 66 | Input file not found or not readable |
| 70 | Internal software error |
| 78 | Configuration error (`EX_CONFIG`) |

## Architecture

Design decisions are recorded in `docs/adr/`.

## Security / supply chain

- CI runs `cargo deny check` on every push and PR to validate licenses,
  advisories, and dependency sources (requires
  [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)). `deny.toml` also
  denies wildcard path dependencies to keep the supply-chain surface explicit.
- The workspace prefers the stdlib when it covers the use case. For example,
  `InMemoryOffloadStore` uses `std::sync::RwLock` instead of an external crate.
- The embedded default config is guarded by a test that asserts it equals
  `PipelineConfig::default()`, preventing silent drift between code and TOML.
- Configuration files are parsed with `#[serde(deny_unknown_fields)]`; unknown
  keys produce a startup error instead of being silently ignored.
- Config errors exit with code 78 so CI and wrappers can branch on them.
- `config --validate --json` emits `{ "valid": true }` on success or
  `{ "valid": false, "error": "..." }` on failure for machine-readable CI checks.
  The error output includes the offending config file path and source kind.

## Security

See [SECURITY.md](SECURITY.md) for the security policy, supported versions,
and how to report vulnerabilities.

## License

This project is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Releasing

The workspace is publishable to crates.io. Because path dependencies between
workspace members carry explicit `version = "0.1.0"` requirements, publish in
dependency order so each crate is available on the registry before the next one
is packaged:

```bash
cargo publish -p kirkstratum-core
cargo publish -p kirkstratum-hosts
cargo publish -p kirkstratum-cli
```

Until `kirkstratum-hosts` is on crates.io, `cargo package -p kirkstratum-cli` cannot be
fully verified because it depends on the registry version of `kirkstratum-hosts`.
The tarball can still be generated locally with `--no-verify`.
