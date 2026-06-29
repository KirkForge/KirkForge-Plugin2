# Stratum Gap Status — 2026-06-29

This file tracks the state of the production/beta gaps from `plugin2-gaps.md` after the hardening pass.

## Completed

| # | Gap | Evidence |
|---|-----|----------|
| 3 | Cargo-deny advisory parse error | Bumped toolchain to 1.88.0 and cargo-deny to 0.19.9; advisory DB now parses and `RUSTSEC-2026-0190` patched via `anyhow 1.0.103`. |
| 4 | Installation instructions | README.md has an **Installation** section with `cargo install stratum`. |
| 5 | Library usage examples | `crates/kirkstratum-core/examples/library_usage.rs` added and compiles. |
| 6 | Cross-platform CI | `.github/workflows/ci.yml` matrix includes Ubuntu x86_64 gnu/musl and macOS aarch64. |
| 7 | Property-based tests | `crates/kirkstratum-core/tests/property.rs` covers no-panic, monotonicity, idempotence, marker round-trip. |
| 8 | Performance benchmarks | `crates/kirkstratum-core/benches/compression.rs` added with Criterion; `--no-run` passes. |
| 9 | Transform execution timeout | `PipelineConfig::transform_timeout_ms` default 30s; `CompressionPipeline::apply_with_timeout` uses `Arc` transforms + start-signal channel + `recv_timeout`. |
| 14 | SECURITY.md hardening | Added supported versions, SLA, and known-limitations sections. |
| 15 | No self-update mechanism | Documented in README/SECURITY.md that users should watch GitHub Releases. |
| 16 | Negative corpus tests | `crates/kirkstratum-core/tests/negative_corpus.rs` covers truncated JSON, invalid UTF-8, corrupted diff, deeply nested braces/brackets, long plain text. |
| 17 | Offload key collision risk | `derive_key` now returns the full 64-character BLAKE3 hex hash. |
| 18 | cargo-semver-checks in CI | Non-blocking `cargo semver-checks check-release` job added to `.github/workflows/ci.yml`. |

## Partially Addressed / Pending External Auth

| # | Gap | State |
|---|-----|-------|
| 1 | Publish to crates.io | `kirkstratum-core` packages cleanly (`cargo package --allow-dirty --locked`). `kirkstratum-hosts`/`kirkstratum-cli` cannot package until their path-dependency `kirkstratum-core` is on crates.io. Needs `CARGO_REGISTRY_TOKEN` and a maintainer to run `cargo publish -p kirkstratum-core && cargo publish -p kirkstratum-hosts && cargo publish -p kirkstratum-cli`. |
| 2 | Release automation | `.github/workflows/release.yml` added with test → publish → cross-platform binaries → SBOM → cosign sign → GitHub Release. Cannot be exercised until the GitHub repo exists. |
| 10 | SQLite offload store | Deferred per ADR; still behind feature gate. No implementation added. |
| 11 | SBOM generation | Implemented in `release.yml` via `cargo-sbom`. Pending a release tag to run. |
| 12 | Binary signing | Implemented in `release.yml` via Sigstore/cosign keyless signing. Pending a release tag. |
| 13 | Feature matrix | README.md has a Features section; auto-generation deferred until feature count grows. |

## Blocked on User Action

- **GitHub remote**: `origin` points to `https://github.com/kirkstratum/stratum.git`, which does not exist. The classifier blocked automated `gh repo create`, so the repository must be created in the GitHub web UI. Once it exists, push with:
  ```bash
  git push -u origin main
  ```
- **crates.io publication**: Requires a crates.io account and `CARGO_REGISTRY_TOKEN` secret in the GitHub repo environment named `crates-io`.

## Verification Results (Rust 1.88.0)

```
cargo +1.88.0 fmt --check          OK
cargo +1.88.0 clippy ... -D warnings OK
cargo +1.88.0 test --workspace --locked --all-features OK (147 tests + 13 doc tests)
RUSTDOCFLAGS="-D warnings" cargo doc OK
cargo +1.88.0 build --profile ci --all-features OK
cargo +1.88.0 bench --no-run --all-features OK
cargo +1.88.0 deny --all-features check OK
```
