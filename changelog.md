# Changelog

## 2026-06-29 — Add per-transform execution timeout
- Added `transform_timeout_ms` to `PipelineConfig` (default 30 000 ms). Each
  content and output transform runs under this deadline; if it expires, the
  transform is skipped and the input to that stage is returned unchanged. A
  value of `0` disables the timeout and runs transforms synchronously.
- Changed `CompressionPipeline` to store transforms as `Arc<dyn Transform>` so
  they can be shared with background worker threads used by the timeout wrapper.
- Added four unit tests for the timeout behavior: default value, disabled
  timeout, slow transform skipped, and fast transform applied.
- Documented the timeout in `README.md` and updated `SECURITY.md` **Known
  limitations** to note that timed-out transforms continue in detached threads.
- Default `config/pipeline.toml` now includes `transform_timeout_ms = 30000`;
  the embedded-TOML roundtrip test guards against drift.

## 2026-06-29 — Harden release workflow and MSRV metadata
- Added waits between crates.io publications in `.github/workflows/release.yml`
  so `kirkstratum-hosts` and `kirkstratum-cli` are only published after their
  dependencies appear in the registry index.
- Removed duplicate artifact uploads from the GitHub Release step in
  `.github/workflows/release.yml` (SBOM and signatures are already collected
  into `final/`).
- Bumped workspace `rust-version` from `1.85` to `1.88.0` to match the pinned
  CI/toolchain version and the actual supported compiler.
- Updated `README.md` **Releasing** section to describe the automated tag-based
  release pipeline and the intentional absence of a self-update mechanism.

## 2026-06-29 — Harden publication, testing, and distribution gaps
- Switched `InMemoryOffloadStore` to use the full 64-character BLAKE3 hash as the
  offload key, eliminating the theoretical prefix-collision risk noted in the
  gaps audit.
- Added property-based tests in `crates/kirkstratum-core/tests/property.rs`
  (`proptest`) covering no-panic, output monotonicity, idempotence, and offload
  marker round-tripping for arbitrary inputs and content types.
- Added negative-corpus tests in `crates/kirkstratum-core/tests/negative_corpus.rs`
  for truncated JSON, invalid UTF-8, corrupted diffs, and deeply nested braces/
  brackets to guard against future transform panics.
- Added a Criterion benchmark in `crates/kirkstratum-core/benches/compression.rs`
  measuring small JSON, large log, large diff, and worst-case plain-text inputs.
- Added a library-usage example in
  `crates/kirkstratum-core/examples/library_usage.rs` showing how host adapters can
  construct a `CompressionPipeline`, `CompressionContext`, and `run()` it.
- Updated `README.md` with an **Installation** section (`cargo install stratum`),
  a **Library usage** section with a code snippet, a **Features** table, and a
  **Performance** section documenting the benchmark harness.
- Updated `.github/workflows/ci.yml` to build and test on a matrix of
  `ubuntu-latest` (x86_64 GNU, x86_64 musl) and `macos-latest` (aarch64), and
  bumped the pinned `cargo-deny` to `0.19.9` (requires Rust 1.88.0) to pick up
  the CVSS 4.0 advisory parser fix and resolve `RUSTSEC-2026-0190`.
- Added `.github/workflows/release.yml` triggered on `v*` tags: test, publish to
  crates.io in dependency order, build release binaries for four targets, generate
  an SBOM with `cargo-sbom`, keyless-sign binaries with Sigstore/cosign, and
  create a GitHub Release with `SHA256SUMS`.
- Expanded `SECURITY.md` with a **Known limitations** section covering the
  process-scoped in-memory store and lack of network isolation beyond the OS
  process model.
- Core unit/integration/property/negative-corpus test count increased; total
  workspace tests now include the original 141 plus 4 property tests and 7
  negative-corpus tests.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features`,
  `cargo build --workspace --locked --all-features --profile ci`,
  `cargo check --benches -p kirkstratum-core --all-features`, and
  `cargo deny check` all pass.

## 2026-06-27 — Expand `#[must_use]` coverage across public traits and CLI helpers
- Completed the `#[must_use]` pass started in Task #150:
  - Added `#[must_use]` to every value-returning method on the `OffloadStore`
    trait (`put`, `get`, `len`, `is_empty`, `backend_name`) so implementors and
    callers inherit the unused-return warning at compile time.
  - Added `#[must_use]` to `Transform::apply` so applying a transform and
    discarding the result is flagged.
  - Added `#[must_use]` to the remaining CLI pure functions where discarding the
    result is a bug: `write_stdout`, `initialise_config`, `read_input`,
    `load_config`, `emit_json_or_human`, `xdg_config_path`, and `ConfigSource::kind`.
    `Result`-returning functions carry a descriptive message to satisfy the
    `clippy::double_must_use` lint while keeping diagnostics specific.
- Removed erroneous `#[must_use]` from `Mode::DEFAULT_MODE` and `Mode::ALL_MODES`
  constants: the attribute has no effect on constants and produced
  `unused_attributes` warnings under `-D warnings`.
- Removed redundant method-level `#[must_use]` from constructors/builders whose
  return types are already covered by struct-level `#[must_use]`
  (`CompressionContext::with_token_budget`, `CompressionContext::with_query`,
  `CompressionPipeline::new`, `InMemoryOffloadStore::new`, `DryRunReport::new`).
- Updated store tests to explicitly discard `put` keys with `let _ = ...` now
  that the trait method is `#[must_use]`.
- No functional behavior changed; total workspace test count remains
  `26 + 34 + 60 + 8 + 13 = 141`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (141 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile ci`,
  and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.
- `cargo deny check licenses` passes; `cargo deny check advisories` cannot run
  locally due to a transient CVSS 4.0 parse error in the cached advisory database
  (unrelated to the workspace).

## 2026-06-27 — Document remaining public CLI items for completeness
- Added rustdoc comments to the previously bare public items in
  `crates/kirkstratum-cli/src/cli.rs` and `crates/kirkstratum-cli/src/main.rs`:
  - `Cli` — top-level command-line argument struct.
  - `Command` — available subcommands enum.
  - `exit::EX_OK`, `EX_USAGE`, `EX_DATAERR`, `EX_NOINPUT`, `EX_SOFTWARE`,
    `EX_CONFIG` — BSD sysexits codes used by the binary.
- This completes the `missing_docs = "deny"` coverage for the CLI crate's
  non-test public surface, making the API fully auditable from generated docs.
- No functional behavior changed; test count remains
  `26 + 34 + 60 + 8 + 13 = 141`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (141 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`), and `cargo deny check licenses/bans/sources` all pass.

## 2026-06-27 — Add `clear` API and ergonomic accessors to `OffloadStore`
- Added a default `clear()` method to the `OffloadStore` trait so backends can
  expose a reset operation without breaking existing implementors. The default
  is a no-op, which is safe for read-only or externally-managed stores.
- Implemented `clear()` on `InMemoryOffloadStore` with the same poison-recovery
  pattern used by `put`, `get`, and `len`, so a long-running host adapter can
  drop accumulated payloads between sessions even if the internal `RwLock` has
  been poisoned.
- Added inherent `len()` and `is_empty()` methods to `InMemoryOffloadStore` so
  callers can inspect the store without importing the `OffloadStore` trait. The
  inherent methods delegate to the trait implementations, keeping behavior
  identical.
- Added `PartialEq` and `Eq` derives to `CompressionContext` so consumers can
  compare contexts (e.g., in tests or caches) without manual field comparisons.
- Added runnable doc tests for `InMemoryOffloadStore::clear` and updated the
  `InMemoryOffloadStore::new` doc test to use the inherent `is_empty()` method.
- Added unit tests `clear_removes_all_payloads`,
  `clear_recover_from_poisoned_lock`, `inherent_len_and_is_empty_delegate_to_trait`,
  and `compression_context_is_equatable`.
- Core unit test count increased from 56 to 60; core doc-test count increased
  from 10 to 11. Total workspace test count is now
  `26 + 34 + 60 + 8 + 13 = 141`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (141 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`), and `cargo deny check licenses/bans/sources` all pass.
- `cargo deny check advisories` cannot run locally due to a transient CVSS 4.0
  parse issue in the cached advisory database, which is unrelated to the
  workspace.

## 2026-06-27 — Add `Debug` impls for public pipeline and store types
- Implemented `std::fmt::Debug` for `CompressionPipeline` in
  `crates/kirkstratum-core/src/pipeline.rs`. The implementation prints the
  content/output transform counts instead of attempting to format the boxed
  closures, keeping logs useful and avoiding exposing arbitrary closure state.
- Implemented `std::fmt::Debug` for `InMemoryOffloadStore` in
  `crates/kirkstratum-core/src/store.rs`. The implementation prints the backend
  name (`"memory"`) and the current length, reusing the existing poison-recovery
  accessor so a debug print never panics on a poisoned lock.
- Added a runnable doc test to `InMemoryOffloadStore::new` showing the basic
  empty-store construction.
- Added unit tests `pipeline_debug_shows_transform_counts` and
  `store_debug_shows_backend_and_len` asserting the debug format carries the
  expected fields.
- Silenced the `clippy::literal_string_with_formatting_args` nursery false
  positive in `crates/kirkstratum-core/src/lib.rs` so extended clippy stays clean.
- Updated test closures added by the debug tests to use `ToString::to_string`
  directly, satisfying the `clippy::redundant_closure_for_method_calls` pedantic
  lint.
- Core unit test count increased from 54 to 56; core doc-test count increased
  from 9 to 10. Total workspace test count is now
  `26 + 34 + 56 + 8 + 12 = 136`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (136 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`), and `cargo deny check licenses/bans/sources` all pass.
- `cargo deny check advisories` cannot run locally due to a transient CVSS 4.0
  parse issue in the cached advisory database, which is unrelated to the
  workspace.

## 2026-06-27 — Expand `#[must_use]` coverage on public API surfaces
- Added `#[must_use]` to public types and functions where discarding the return
  value is almost certainly a bug: `CompressionContext`, `CompressionPipeline`,
  `InMemoryOffloadStore`, `CompressionPipeline::run`,
  `ContentTypeParseError::new`, `ModeParseError::new`,
  `filter_deactivated_tags`, `DryRunReport::to_json`, `DryRunReport::human`,
  `InputTooLarge::new`, `max_input_size`, `ConfigSource::to_human`, and
  `resolve_mode_with_override`.
- Removed redundant `#[must_use]` from constructors and builder methods whose
  return types are already covered by struct-level `#[must_use]`, avoiding the
  `clippy::double_must_use` lint.
- This hardens the library API for downstream consumers by surfacing accidental
  misuse at compile time instead of silently dropping computed values.
- Total workspace test count remains `26 + 34 + 54 + 8 + 11 = 133`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (133 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`), and `cargo deny check licenses/bans/sources` all pass.

## 2026-06-27 — Add `build.rs` rerun-if-changed for embedded assets
- Added `build.rs` to `kirkstratum-core` so edits to `config/pipeline.toml`
  automatically trigger a rebuild of crates that embed it via `include_str!`.
- Added `build.rs` to `kirkstratum-hosts` so edits to
  `docs/rules/CANONICAL.md` automatically trigger a rebuild of crates that
  embed the canonical ruleset.
- This prevents stale binaries after config or ruleset tuning and removes the
  need for a manual `cargo clean` when iterating on embedded assets.
- Total workspace test count remains `26 + 34 + 54 + 8 + 11 = 133`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (133 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`), and `cargo deny check licenses/bans/sources` all pass.
- Confirmed manually that `touch crates/kirkstratum-core/config/pipeline.toml`
  and `touch crates/kirkstratum-hosts/docs/rules/CANONICAL.md` each cause the
  expected crate rebuild on the next `cargo build`.

## 2026-06-27 — Fix CI cargo-deny version to match Rust 1.85 toolchain
- Changed the CI installation of `cargo-deny` from `0.19.9` to `0.18.3`, the
  latest version that builds on the Rust 1.85.0 toolchain pinned in the same
  workflow.
- `cargo-deny 0.19.9` requires Rust 1.88.0 or newer, so the previous CI job would
  fail during the install step. The `0.18.3` version is compatible and the
  license, ban, and source checks all pass locally (the advisory database has a
  transient CVSS 4.0 parse issue in the local cache, which is unrelated to the
  workspace).
- Total workspace test count remains `26 + 34 + 54 + 8 + 11 = 133`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (133 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Commit `Cargo.lock` for reproducible builds
- Removed `Cargo.lock` from `.gitignore` and staged the existing lockfile.
- Cargo recommends committing the lockfile for workspaces that produce binary
  artifacts (here the `stratum` CLI). This makes CI builds, local builds, and
  release builds deterministic and avoids accidental dependency drift.
- The lockfile was already up to date (`cargo build --workspace --locked` passes),
  so no dependency versions changed.
- Total workspace test count remains `26 + 34 + 54 + 8 + 11 = 133`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (133 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Add `CompressionContext::with_query` builder
- Added `with_query` to `CompressionContext` so the public `query` field can be
  set via the same fluent builder style already used for `with_token_budget`.
- This completes the builder surface for the context type and makes it easier
  for host adapters and future transforms to supply a relevance query without
  mutating the field directly.
- Added unit tests `with_query_sets_optional_query_string` and
  `with_query_and_token_budget_chain`, plus a doc test for the new method.
- Total workspace test count is now `26 + 34 + 54 + 8 + 11 = 133`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (133 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Make `config --validate` and `config --sources` mutually exclusive
- Declared `validate` and `sources` as mutually exclusive in the `config`
  subcommand using clap's `conflicts_with` attribute.
- This prevents silently ignoring `--sources` when `--validate` is also supplied;
  the user now gets a clear usage error (exit code 64) naming both flags.
- Added integration test `config_validate_and_sources_are_mutually_exclusive`
  asserting the conflict is reported with exit code 64 and mentions both flags.
- Total workspace test count is now `26 + 34 + 52 + 8 + 10 = 130`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (130 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Validate `completion` shell at parse time and avoid consuming `Cli` in `run`
- Changed `Command::Completion { shell: String }` to `Command::Completion { shell: clap_complete::Shell }` so clap validates the shell argument at parse time using `clap_complete::Shell`'s `ValueEnum` implementation.
- This gives users the supported shell list in `--help`, rejects unknown shells before dispatch (still exiting with code 64), and removes the manual `parse_shell` helper and its hard-coded shell list.
- Changed `run(cli: Cli)` to `run(cli: &Cli)` so the CLI struct is no longer consumed by dispatch; subcommand fields are dereferenced where `Copy` and cloned where needed. This clears the `needless_pass_by_value` pedantic lint and makes the dispatch function cheaper to call.
- Total workspace test count remains `26 + 33 + 52 + 8 + 10 = 129`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (129 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Mark public enums as `#[non_exhaustive]` for API stability
- Added `#[non_exhaustive]` to the public enums that downstream code is most
  likely to match on or extend:
  - `kirkstratum-core::Mode`
  - `kirkstratum-core::ContentType`
  - `kirkstratum-core::ConfigError`
  - `kirkstratum-cli::ConfigSource`
- This prevents a future variant addition from becoming a silent breaking change
  for consumers that match these enums exhaustively, which is standard practice
  for production Rust libraries. Internal code and tests in the defining crate are
  unaffected, and the existing variant-specific `matches!` assertions continue to
  compile.
- Total workspace test count remains `26 + 33 + 52 + 8 + 10 = 129`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (129 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Make parse-error messages derive their variant lists dynamically
- Changed `ModeParseError` and `ContentTypeParseError` display messages from
  hand-written literal lists (`"off, lite, full, ultra"` and the eight content
  types) to lists generated at format time from `Mode::ALL_MODES` and
  `ContentType::ALL`.
- This means adding a new `Mode` or `ContentType` variant automatically updates
  the user-facing error message and the CLI `--help` parser, eliminating a
  common source of stale error text.
- No public API or test assertions changed; the existing
  `unknown_mode_error_lists_supported_modes` and
  `unknown_content_type_error_lists_supported_types` tests continue to pass.
- Total workspace test count remains `26 + 33 + 52 + 8 + 10 = 129`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (129 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Make `Mode::ALL_MODES` a sized array and add a `Lite` rules test
- Changed `ALL_MODES` from `&[Mode]` to `[Mode; 4]` so it is a sized value with
  no hidden allocation/reference, matching `ContentType::ALL` and making it
  usable in const contexts.
- Updated the CLI mode value parser to iterate `ALL_MODES` instead of a
  hand-written literal list, so adding a new `Mode` variant automatically
  updates the supported CLI values.
- Renamed and split the ambiguously-named `lite_mode_keeps_only_all_and_full`
  test into `lite_mode_keeps_only_all` and `full_mode_keeps_all_and_full`,
  covering both `Mode::Lite` and `Mode::Full` behavior in the canonical rules
  filter. Host rules test count increased from 7 to 8.
- Total workspace test count is now `26 + 33 + 52 + 8 + 10 = 129`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (129 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Add `SECURITY.md` and explicit crate-level `unsafe_code` forbids
- Added `SECURITY.md` documenting the supported versions policy, vulnerability
  reporting process, response-time expectations, the Stratum security model, and
  the CI supply-chain checks. This is a standard enterprise security policy
  deliverable and gives security researchers a clear path for responsible
  disclosure.
- Added `#![forbid(unsafe_code)]` to the crate roots of `kirkstratum-core`,
  `kirkstratum-hosts`, and `kirkstratum-cli`. The workspace lint already
  forbade unsafe code, but the explicit per-crate attributes provide
  defense-in-depth and make the safety boundary visible when auditing a single
  crate in isolation.
- Added a "Security" section to `README.md` pointing to `SECURITY.md`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (`26 + 33 + 52 + 7 + 10 = 128` tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, `cargo build --workspace --locked
  --all-features --profile ci`, and extended clippy (`-W clippy::pedantic -W
  clippy::nursery -W clippy::cargo`) all pass.

## 2026-06-27 — Make `InMemoryOffloadStore` resilient to lock poisoning
- Replaced the three `RwLock::expect(... "offload store lock is never poisoned")`
  calls in `InMemoryOffloadStore` with explicit poison recovery via
  `PoisonError::into_inner`. The store now logs a warning and continues operating
  instead of panicking if the internal lock is poisoned.
- This hardens the pipeline against third-party transforms (or future code
  paths) that might panic while the store lock is held, keeping `stratum run`
  available in production instead of aborting on a recoverable mutex state.
- Added `store_recovers_from_poisoned_lock` in `crates/kirkstratum-core/src/store.rs`
  that deliberately poisons the lock and asserts `put`, `get`, and `len` all
  recover and return correct values. Core test count increased from 51 to 52;
  the total workspace test count is now `26 + 33 + 52 + 7 + 10 = 128`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (128 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Add runnable doc tests to the `kirkstratum-hosts` public API
- Added `# Examples` doc tests to `filter_by_mode` in
  `crates/kirkstratum-hosts/src/rules.rs` and to `build_rules` in
  `crates/kirkstratum-hosts/src/lib.rs`.
- These examples exercise the host-rules filtering contract directly from
  rustdoc, giving consumers copy-pasteable usage samples and guarding the
  public API against accidental semantic drift.
- `kirkstratum-hosts` doc-test count increased from 0 to 2; the total
  workspace test count is now `26 + 33 + 51 + 7 + 10 = 127`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (127 tests), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --all-features --profile
  ci`, and extended clippy (`-W clippy::pedantic -W clippy::nursery -W
  clippy::cargo`) all pass.

## 2026-06-27 — Update ADR code examples to use the renamed crate names
- Updated the design-record code examples in `docs/adr/0008-cross-host-adapters.md`,
  `docs/adr/0009-output-shim.md`, and `docs/adr/0010-hooks-integration.md` to
  reference the current crate names (`kirkstratum-core`, `kirkstratum-hosts`)
  instead of the old names (`stratum-core`, `stratum-hosts`).
- This keeps the documented architecture in sync with the actual workspace and
  avoids confusion for new contributors reading the ADRs after the rename.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 33 + 51 + 7 + 8 = 125 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, and `cargo build --workspace --locked
  --all-features --profile ci` all pass.

## 2026-06-27 — Align `Mode::offload_threshold` with the `f64` ratio API
- Changed `Mode::offload_threshold` from `Option<f32>` to `Option<f64>` so the
  preferred bloat thresholds returned by mode helpers use the same precision as
  `Ratio`, `PipelineConfig::bloat_threshold_for`, and the bloat-ratio
  heuristic. This removes a latent type inconsistency and avoids silent `f32`
  truncation if a consumer ever uses the mode threshold directly in a comparison
  with config-derived `f64` values.
- The literal values (`0.8`, `0.5`, `0.2`) and the existing
  `mode_offload_threshold_matches_documented_values` test continue to pass
  without modification; Rust now infers them as `f64`.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 33 + 51 + 7 + 8 = 125 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, `cargo build --workspace --locked
  --all-features --profile ci`, and extended clippy (`-W clippy::pedantic -W
  clippy::nursery -W clippy::cargo`) all pass.

## 2026-06-27 — Align `deny.toml` license allow-list with the real dependency graph
- Updated `deny.toml` to include the missing permissive licenses used by the
  current transitive dependency tree:
  - `BSD-2-Clause` (used by `arrayref`, a transitive dependency of `blake3`).
  - `Unicode-3.0` (used by `unicode-ident`, a proc-macro stack dependency).
- Kept the existing `BSD-3-Clause`, `ISC`, `MPL-2.0`, and `Unicode-DFS-2016`
  entries, and sorted the allow-list alphabetically for readability.
- The previous allow-list only covered the directly-declared workspace crates;
  running `cargo deny check` with `cargo-deny 0.19.9` in CI would have failed on
  `unicode-ident` and `arrayref`. This removes that publishing/CI blocker.
- Verified every transitive license expression is now satisfiable using a
  `cargo tree --format '{p} {l}'` audit: every crate has at least one allowed
  license alternative, and the `AND` requirement for `Unicode-3.0` is satisfied.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 33 + 51 + 7 + 8 = 125 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, and `cargo build --workspace --locked
  --all-features --profile ci` all pass.
- Note: local `cargo-deny` is `0.16.1`, which does not understand Rust edition
  2024, so `cargo deny check` cannot be executed locally until the tool is
  upgraded to `0.19.9` (the version pinned in CI).

## 2026-06-27 — Rename workspace crates to avoid crates.io name collisions
- Renamed the workspace crates from `stratum-core`, `stratum-hosts`, and
  `stratum-cli` to `kirkstratum-core`, `kirkstratum-hosts`, and `kirkstratum-cli`.
  The `stratum` binary name is unchanged.
- Motivation: `stratum-core` collides with an existing crates.io crate (the
  Stratum V2 mining protocol hub, versions 0.1.0 and 0.4.0). Cargo's package
  verification was resolving the dependency against the wrong crate, causing
  `stratum-hosts` packaging to fail with `unresolved import stratum_core::mode`.
  The `kirkstratum-*` prefix matches the GitHub org namespace and is available
  on crates.io, removing the publishing blocker.
- Updated workspace member paths, package names, path dependencies, Rust imports,
  and all documentation (README, plugin README, ADRs, CANONICAL.md, and historical
  changelog entries) to use the new names consistently.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 33 + 51 + 7 + 8 = 125 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, and `cargo build --workspace --locked
  --all-features --profile ci` all pass.
- Verified `cargo package -p kirkstratum-core --allow-dirty` now packages and
  builds cleanly from the crate tarball. `kirkstratum-hosts` and `kirkstratum-cli`
  still require `kirkstratum-core` to be published to crates.io before their
  package verification can resolve dependencies; their tarballs generate correctly
  once the registry dependency is available.

## 2026-06-26 — Align agent-hooks manifest with the implemented CLI
- Updated `plugins/stratum-agent-hooks/hooks/hooks.json` so the plugin lifecycle
  events invoke CLI commands that actually exist, instead of the ADR-0010
  subcommands (`stratum init hook ensure`, `stratum rules emit`,
  `stratum mode track`) which are not yet implemented:
  - `SessionStart` now runs `stratum rules` to inject the canonical ruleset.
  - `PreToolUse` now runs `stratum config --validate` to verify the runtime
    config is healthy before a tool call proceeds.
  - `UserPromptSubmit` now runs `stratum run` to compress the submitted prompt
    through the pipeline.
- Added four integration tests in `crates/kirkstratum-cli/tests/integration.rs`
  exercising the hook command contract: session-start rules emission,
  pre-tool-use config validation (both success and failure), and user-prompt
  pipeline execution. These tests guard against future CLI changes breaking the
  plugin manifest.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 33 + 51 + 7 + 8 = 125 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, and `cargo build --workspace --locked
  --all-features --profile ci` all pass.

## 2026-06-26 — Add MIT and Apache-2.0 license files
- Added `LICENSE-MIT` and `LICENSE-APACHE` to the repository root so the declared
  dual license (`MIT OR Apache-2.0` in `[workspace.package]`) is accompanied by
  the actual license texts.
- Added a "License" section to `README.md` pointing to both files. This makes
  the licensing terms discoverable for enterprise users and satisfies the
  common expectation of crates.io consumers that dual-licensed Rust crates ship
  the corresponding license files.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 29 + 51 + 7 + 8 = 121 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, and `cargo build --workspace --locked
  --all-features --profile ci` all pass.

## 2026-06-26 — Centralize CLI-only runtime dependencies in workspace dependencies
- Moved `tracing-subscriber` and `clap_complete` from inline version specs in
  `crates/kirkstratum-cli/Cargo.toml` into `[workspace.dependencies]` in the root
  `Cargo.toml`. `kirkstratum-cli` now references them with `{ workspace = true }`.
- This keeps all dependency versions in one place, prevents drift between crates,
  and makes supply-chain audits and version bumps easier.
- Added a "Releasing" section to `README.md` documenting the crates.io publish
  order (`kirkstratum-core` → `kirkstratum-hosts` → `kirkstratum-cli`) and the `--no-verify`
  caveat for `kirkstratum-cli` packaging until `kirkstratum-hosts` is published.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 29 + 51 + 7 + 8 = 121 tests), `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --all-features`, and `cargo build --workspace --locked
  --all-features --profile ci` all pass.

## 2026-06-26 — Align CI with the full verification suite and update cargo-deny
- Updated `.github/workflows/ci.yml` so the `Test` step runs `cargo test
  --workspace --locked --all-features` (was `--workspace --locked`) and the
  `Docs` step runs `cargo doc --workspace --no-deps --all-features` (was
  `--workspace --no-deps`). This makes CI exercise the same feature surface as
  the local full verification suite and prevents feature-gated doc/tests from
  being skipped.
- Bumped the pinned `cargo-deny` install from `0.16.1` to `0.19.9`. The previous
  pin fails on the current dependency graph because it cannot parse
  `edition = "2024"` in upstream crate manifests; `0.19.9` supports edition
  2024 while keeping the reproducibility benefit of a pinned, `--locked`
  install.
- Updated the `README.md` "Test" section to list the same full verification
  commands used locally and in CI: `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets --all-features`, `cargo test --workspace
  --all-features`, `cargo doc --workspace --all-features --no-deps`, plus a
  release-equivalent `cargo build --workspace --locked --all-features --profile
  ci` example.
- Verified `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features --locked`, `cargo test --workspace --locked --all-features`
  (26 + 29 + 51 + 7 + 8 = 121 tests), `cargo test --workspace --all-features
  --release`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  --all-features`, `cargo build --workspace --locked --profile ci`, and
  extended clippy (`-W clippy::pedantic -W clippy::nursery -W clippy::cargo`)
  all pass. The `cargo deny check` step itself cannot be run locally in this
  environment because `cargo-deny 0.19.9` is not installed, but the version bump
  directly addresses the known edition-2024 parsing failure observed with
  `0.16.1`.

## 2026-06-26 — Make workspace path dependencies crates.io-compatible and fix `kirkstratum-hosts` packaging
- Added explicit `version = "0.1.0"` to every intra-workspace path dependency so
  `cargo package` and `cargo publish` can resolve them against crates.io once the
  crates are published:
  - `kirkstratum-cli` now declares `kirkstratum-core` and `kirkstratum-hosts` with both
    `path` and `version`.
  - `kirkstratum-hosts` now declares `kirkstratum-core` with both `path` and `version`.
- Moved `docs/rules/CANONICAL.md` into `crates/kirkstratum-hosts/docs/rules/CANONICAL.md`
  and updated the `include_str!` invocation to use
  `concat!(env!("CARGO_MANIFEST_DIR"), "/docs/rules/CANONICAL.md")`. This makes
  the canonical ruleset part of the `kirkstratum-hosts` crate package, so `cargo package`
  can build the crate tarball without referring to files outside the package root.
- Updated ADR-0006 and ADR-0008 to reference the new canonical-ruleset location.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (26 + 29 + 51 + 7 + 8 = 121 tests),
  `cargo test --workspace --all-features --release`, `cargo doc --workspace
  --all-features --no-deps`, and `cargo build --workspace --locked --all-features`
  all pass. Also verified `cargo package --allow-dirty -p kirkstratum-core` succeeds and
  `cargo package --allow-dirty -p kirkstratum-hosts --no-verify --offline` produces a
  valid crate. `cargo package -p kirkstratum-cli` cannot yet be verified because it
  depends on `kirkstratum-hosts`, which must first be published to crates.io; once
  `kirkstratum-hosts` is available, the versioned path dependency will resolve cleanly.

## 2026-06-26 — Move CLI binary tests into a dedicated integration test crate
- Extracted the 28 `assert_cmd` binary tests from `crates/kirkstratum-cli/src/cli.rs`
  into a new `crates/kirkstratum-cli/tests/integration.rs` integration test file.
- This is the idiomatic location for binary-level tests: integration tests have
  access to the `CARGO_BIN_EXE_stratum` env var that `assert_cmd::cargo_bin`
  needs, which was previously unavailable when running tests under `--release`
  or outside a Cargo-managed binary build.
- The move removes the `assert_cmd`, `predicates`, and `tempfile` dev-dependency
  imports from the in-module test block and keeps library unit tests focused on
  the `Cli` parsing and config loading logic.
- Added a crate-level doc comment to the new test file so the `missing_docs = "deny"`
  workspace lint remains satisfied.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (26 + 29 + 51 + 7 + 8 = 121 tests), and
  `cargo test --workspace --all-features --release` all pass. Also verified
  `cargo doc --workspace --all-features --no-deps` and `cargo build --workspace
  --locked --all-features` pass.

## 2026-06-26 — Make `Mode` helper methods const and use explicit patterns
- Made `Mode::runs_transforms` and `Mode::offloads_bloat` `const fn` so callers
  can use them in const contexts and the compiler can evaluate them at compile
  time.
- Replaced the negated inequalities (`self != Self::Off`) with explicit positive
  patterns (`!matches!(self, Self::Off)` and `matches!(self, Self::Full |
  Self::Ultra)`). This more directly encodes the intended mode sets, is easier
  to extend when new modes are added, and keeps the helper methods in the same
  style as `as_str`/`offload_threshold`.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic
  -W clippy::nursery -W clippy::cargo`, `cargo test --workspace --all-features`
  (55 + 51 + 7 + 8 = 121 tests), and `cargo doc --workspace --all-features
  --no-deps` all pass.

## 2026-06-26 — Add crates.io package metadata to all workspace crates
- Added shared `description`, `readme`, and `keywords` to `[workspace.package]`
  in the root `Cargo.toml` so every crate can inherit them.
- Updated each crate (`kirkstratum-core`, `kirkstratum-cli`, `kirkstratum-hosts`) to inherit
  `description`, `readme`, `repository`, and `keywords` from the workspace and
  declare appropriate `categories`:
  - `kirkstratum-core`: `text-processing`, `development-tools`
  - `kirkstratum-cli`: `command-line-utilities`, `development-tools`
  - `kirkstratum-hosts`: `development-tools`
- This satisfies the `clippy::cargo` package-metadata lints and makes the
  workspace ready for publishing to crates.io.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo clippy --workspace --all-targets --all-features -- -W clippy::cargo`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 8 = 121 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass.

## 2026-06-26 — Silence final pedantic/nursery clippy warnings in `bloat_ratio`
- Added an explicit `#[allow(clippy::cast_precision_loss)]` attribute on
  `CompressionContext::bloat_ratio` together with a precision note in the doc
  comment. The `usize` to `f64` casts are intentional: this is a coarse bloat
  heuristic, and inputs beyond 2^53 bytes do not need exact byte counts.
- This removes the last two recurring pedantic/nursery clippy warnings, making
  the codebase clean under `-W clippy::pedantic -W clippy::nursery` while
  keeping the default `cargo clippy --workspace --all-targets --all-features`
  run warning-free.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 8 = 121 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass. Pedantic/nursery
  clippy now reports zero warnings.

## 2026-06-26 — Add doctest for `CompressionPipeline::register_output_transform`
- Added a runnable `doc-test` example to `CompressionPipeline::register_output_transform`
  showing how to register a simple text transform (`replace('a', "A")`).
- This keeps the public pipeline API documented by example and increases the
  kirkstratum-core doc-test count from 7 to 8.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 8 = 121 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass. Pedantic/nursery
  clippy now reports only the two expected `usize` to `f64` precision warnings
  in `bloat_ratio`.

## 2026-06-26 — Simplify build-output detection markers
- Replaced the hand-rolled chain of `starts_with` checks for build-output
  markers (`error[`, `error:`, `warning:`, `test result:`) with an inline
  marker array and a single `iter().any()` pass, mirroring the earlier
  source-code token cleanup. This makes the marker list easier to extend and
  audits more uniform.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 7 = 120 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass. Pedantic/nursery
  clippy now reports only the two expected `usize` to `f64` precision warnings
  in `bloat_ratio`.

## 2026-06-26 — Simplify source-code detection tokens
- Replaced the hand-rolled chain of `starts_with` checks in
  `detect_content_type` with an inline array of structural tokens and a single
  `iter().any()` pass. This makes adding new language markers a one-line change
  and removes the risk of forgetting to call `trim_start()` on a new token
  variant.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 7 = 120 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass. Pedantic/nursery
  clippy now reports only the two expected `usize` to `f64` precision warnings
  in `bloat_ratio`.

## 2026-06-26 — Remove redundant closure in `bloat_threshold_for`
- Replaced the redundant closure `|r| r.get()` with the method reference
  `Ratio::get` in `PipelineConfig::bloat_threshold_for`. This satisfies the
  `clippy::redundant_closure_for_method_calls` pedantic lint and makes the
  fallback mapping slightly more concise.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 7 = 120 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass. Pedantic/nursery
  clippy now reports only the two expected `usize` to `f64` precision warnings
  in `bloat_ratio`.

## 2026-06-26 — Add `PipelineConfig::bloat_threshold_for` accessor
- Added `PipelineConfig::bloat_threshold_for(content_type)` to return the
  effective bloat threshold for a given content type, falling back to the global
  threshold when no per-domain override exists. This centralizes the
  precedence logic that was previously duplicated in `CompressionContext::is_bloated`.
- Updated `CompressionContext::is_bloated` to use the new accessor, making the
  bloat decision a single `cfg.bloat_threshold_for(content_type)` call.
- Added unit tests covering the global fallback and per-domain override cases.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 51 + 7 + 7 = 120 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass.

## 2026-06-26 — Document why unmatched private/public markers drop the remainder
- Added an inline comment in `remove_delimited_sections` explaining that when a
  `<!-- private -->` or `<!-- public -->` section is opened without a matching
  closing marker the rest of the input is intentionally dropped. The comment
  clarifies that this default prevents leaking content that was explicitly
  marked private/public, making the security-relevant behavior obvious during
  audits rather than looking like a bug.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 49 + 7 + 7 = 118 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass.

## 2026-06-26 — Make `ConfigError` accessors const-correct
- Marked all six `ConfigError` accessors as `const fn`. This is a small
  production ergonomics win: callers can now use the accessors in const contexts
  and the compiler is free to evaluate them at compile time when the error is
  statically known.
- Adjusted the return types of `io_path()` and `parse_path()` to
  `Option<&std::path::PathBuf>` so they are const-compatible on the pinned
  toolchain without relying on conditionally-const `Deref` coercion.
- Adjusted `invalid_field()` and `invalid_message()` to return
  `Option<&String>` for the same reason, preserving the ability to compare
  against owned strings in tests.
- Updated the accessor tests to compare against the adjusted return types.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 49 + 7 + 7 = 118 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass. Pedantic/nursery
  clippy now reports only the two expected `usize` to `f64` precision warnings
  in `bloat_ratio`.

## 2026-06-26 — Add `Display` and human-readable labels for `ContentType`
- Added `impl Display for ContentType` so the canonical `snake_case` name can
  be emitted with plain `{}` formatting, matching the existing `Display` impl
  for `Mode` and making report/CLI output more idiomatic.
- Added `ContentType::label()` returning a stable human-readable string (e.g.
  `"source code"`, `"json array"`) with underscores replaced by spaces. Report
  consumers and the dry-run output now use this method directly instead of
  repeatedly allocating via `as_str().replace('_', " ")`.
- Updated `DryRunReport::to_json` and `DryRunReport::human` to use `label()`,
  removing two per-report `String` allocations and ensuring the label is
  centralized in one `&'static str` table.
- Added unit tests covering `Display` round-tripping with `as_str()` and the
  full set of `label()` values.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 46 + 7 + 7 = 115 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass.

## 2026-06-26 — Avoid a clone in `Mode` and `ContentType` parsing
- Refactored `Mode::from_str` and `ContentType::from_str` to bind the normalized
  input to a single local (`normalized`) instead of re-constructing it inside the
  error arm. This removes one redundant `to_ascii_lowercase` allocation on every
  unknown-value path and makes the function read more naturally.
- Replaced the wildcard match arm (`other`) with `_` so it consumes the already
  normalized `String` directly, ensuring `ContentTypeParseError::value` and
  `ModeParseError::value` report the trimmed, lowercased input as intended.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 44 + 7 + 7 = 113 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass.

## 2026-06-26 — Add `PipelineConfig::overrides_for` and document embedded-default expect
- Added `PipelineConfig::overrides_for(content_type)` so consumers can look up
  per-domain overrides without reaching into the public `per_domain` HashMap.
  The pipeline now uses this accessor, removing the only direct `.get()` on
  `per_domain` outside of tests and making the override lookup a stable API.
- Added unit tests for `overrides_for` covering the empty and populated cases.
- Added a `SAFETY` comment above the `PipelineConfig::default()` expect,
  explaining that `DEFAULT_TOML` is a checked-in build artifact and panicking on
  corruption is the correct response. This matches the codebase convention for
  documenting why an `expect` is justified in production code.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features` (55 + 44 + 7 + 7 = 113 tests), and
  `cargo doc --workspace --all-features --no-deps` all pass.

## 2026-06-26 — Make parse errors structured and comparable
- Converted `ModeParseError` and `ContentTypeParseError` from public tuple
  structs (`pub struct X(pub String)`) into named-field structs with private
  storage and a public `value()` accessor.
- Added `Clone`, `PartialEq`, and `Eq` derives to both error types so callers
  can store, compare, and test them without string-parsing the display output.
- Added public constructors (`ModeParseError::new`, `ContentTypeParseError::new`)
  and used them at the `FromStr` error sites, removing direct struct-literal
  construction and making the error creation consistent with `InputTooLarge`.
- Added dedicated tests verifying that the unknown value is exposed and that
  the types are cloneable and equatable.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features`, and `cargo doc --workspace
  --all-features --no-deps` all pass. Pedantic/nursery clippy still reports
  only the two expected `usize` to `f64` precision warnings in `bloat_ratio`.

## 2026-06-26 — Make `InputTooLarge` a richer error type
- Added `Clone`, `Copy`, `PartialEq`, and `Eq` derives to `InputTooLarge` so it can
  be stored, compared, and used in const contexts.
- Added public `InputTooLarge::new`, `limit`, and `got` accessors so callers that
  downcast the error can inspect the configured limit and observed size
  programmatically instead of parsing the display string.
- Switched the internal construction sites in `read_input` to use the new
  constructor, removing direct struct-literal construction and making the
  error creation more maintainable.

## 2026-06-26 — Derive `Clone` and `PartialEq` for `DryRunReport`
- Added `Clone` and `PartialEq` derives to `DryRunReport`. This lets consumers
  store, compare, and test dry-run reports without manual plumbing, which is
  common in CI/enterprise wrappers that want to assert on the planned pipeline
  behavior before applying it.

## 2026-06-26 — Make panic hook resilient to broken stderr
- Replaced the bare `eprintln!("stratum: internal error: {payload}")` in the panic
  hook with a `print_panic_message` helper that locks stderr once and ignores
  `BrokenPipe`. This completes the broken-pipe hardening across all stderr write
  paths (clap errors, top-level errors, and panics), so a panic inside a pipeline
  does not abort the process just because stderr was closed early.

## 2026-06-26 — Make error-message printing resilient to broken stderr
- Replaced the bare `eprintln!("stratum: {e:#}")` in the top-level error path
  with a `print_error_message` helper that locks stderr once and ignores
  `BrokenPipe`, mirroring the existing broken-pipe handling for stdout and clap
  errors. This prevents `stratum run 2>&1 | head` (and similar pipelines) from
  panicking when the downstream reader closes early.

## 2026-06-26 — Document the infallible `write!` in `derive_key`
- Added an explanatory comment in `store.rs` next to the `let _ = write!(out, ...)`
  call, clarifying that `write!` to a `String` only fails on out-of-memory, which
  aborts the process on the current Rust target, so the result is effectively
  infallible. This removes a subtle maintenance footgun for readers auditing
  ignored results.

## 2026-06-26 — Add `Default` impl for `CompressionPipeline`
- Added `impl Default for CompressionPipeline` delegating to `CompressionPipeline::new()`.
  This satisfies the `clippy::new_without_default` lint and lets callers construct an
  empty pipeline via `CompressionPipeline::default()` in addition to `::new()`, which is
  convenient for struct initialization (`..Default::default()`) and generic contexts.
- `#[must_use]` remains on the struct; the `new()` constructor no longer carries a
  redundant `#[must_use]` attribute (the type-level marker is sufficient).
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features`, and `cargo doc --workspace --all-features
  --no-deps` all pass.

## 2026-06-26 — Simplify copyable enum match patterns in `main.rs`
- Changed `Command::Mode { ref value }` and `Command::Rules { ref mode }` match
  arms to bind by value (`value`, `mode`) since `Option<Mode>` is `Copy`. This
  removes unnecessary reference indirection and aligns with idiomatic Rust for
  small `Copy` payloads.

## 2026-06-26 — Harden `init` config directory permissions on Unix
- `initialise_config` now sets the target config directory to `0o700` on Unix
  in addition to the existing `0o600` file permissions. This prevents other users
  from listing or traversing the directory that contains host-specific tuning,
  closing a gap where the file was private but its parent path remained
  world-searchable under typical umasks.
- Updated `init_config_creates_xdg_file` and `init_config_writes_to_config_dir_when_given`
  to assert the directory mode as well as the file mode on Unix.
- Verified `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features`, and `cargo doc --workspace --all-features
  --no-deps` all pass.

## 2026-06-26 — Extend `#[must_use]`, `# Errors` docs, and const-fn hygiene
- Added `#[must_use]` to public pure/projection methods across the workspace so
  ignored return values surface at compile time:
  - `kirkstratum-core`: `Ratio::new_unchecked`, `Ratio::get`, `ContentType::as_str`,
    `detect_content_type`, `remove_private_sections`, `remove_public_sections`,
    `Mode::as_str`, `Mode::runs_transforms`, `Mode::offload_threshold`,
    `Mode::offloads_bloat`, `CompressionContext::is_bloated`,
    `CompressionContext::bloat_ratio`, `CompressionContext::with_token_budget`,
    `InMemoryOffloadStore::new`.
  - `kirkstratum-hosts`: `filter_by_mode`, `build_rules`.
- Added `# Errors` documentation to `PipelineConfig::from_toml` and
  `PipelineConfig::from_file` so consumers know which error conditions each
  function can return.
- Replaced the type-name repetition in `ContentType::ALL` with `Self` inside the
  `impl` block (`[Self; 8]`), matching the existing `Self::` pattern for variants.
- Made the `kirkstratum-cli` test helper `cli_with_mode` a `const fn`.
- Verified `cargo clippy --workspace --all-targets --all-features`,
  `cargo test --workspace --all-features`, and `cargo doc --workspace
  --all-features --no-deps` remain clean. `cargo deny check` is left to CI
  because `cargo-deny` is not installed in this environment.

## 2026-06-26 — Convert `ContentType::ALL` to an array and simplify `read_input`
- Changed `ContentType::ALL` from `&[ContentType]` to `[ContentType; 8]` so it
  is a sized value with no hidden allocation/reference, and updated the roundtrip
  test to iterate by value instead of by reference.
- Replaced the `match file { Some(p) => ... None => ... }` structure in
  `read_input` with an early `if let Some(p) = file` guard. This removes the
  two-arm match over a single scrutinee and lets the rest of the function read
  stdin at top level, improving readability.
- Removed the now-redundant `Ok(...)?` wrapper on `String::from_utf8` results,
  cleaning up the clippy `needless_question_mark` warning.
- Collapsed identical `NotFound`/`PermissionDenied` match arms in the CLI
  `exit_code` mapping.
- Switched remaining single-character string patterns to `char` patterns in
  `kirkstratum-hosts` tests.

## 2026-06-26 — Resolve remaining pedantic clippy warnings
- Made `resolve_mode_with_override` a `const fn` by rewriting the `Option::or`
  chain as explicit `match` arms.
- Replaced a `find(...).map(...).unwrap_or(...)` chain in
  `filter_deactivated_tags` with `find(char::is_whitespace).map_or(...)`.
- Replaced `.map(...).unwrap_or_else(...)` in `CompressionContext::is_bloated`
  with `Option::map_or`.
- Replaced strict `assert_eq!` on `f64` results in tests with
  `assert!((actual - expected).abs() < f64::EPSILON)` so the assertions are
  numerically robust.
- Replaced single-character string patterns with `char` patterns in
  `kirkstratum-hosts` rule tests.
- Removed redundant `&'static` in `ContentType::ALL`; the type is inferred as
  `&[ContentType]` and still has a `'static` lifetime.

## 2026-06-26 — Mark small pure helper methods as `const fn`
- Made `Ratio::get`, `ContentType::as_str`, `Mode::as_str`,
  `Mode::offload_threshold`, `ConfigSource::kind`, and the test-only
  `InputTooLarge::new` `const fn` where possible. These are pure value
  projections and can now be used in const contexts, slightly reducing runtime
  overhead and improving API ergonomics.
- Switched `ConfigSource::kind` and `ConfigSource::to_human` match arms to use
  `Self::` for consistency with the rest of the codebase.

## 2026-06-26 — Apply additional clippy::pedantic hygiene fixes
- Removed redundant closures and clones in `main.rs`, `pipeline.rs`, and
  `config_source.rs` tests.
- Switched remaining `format!("... {}", x)` calls to inline format-capture
  (`format!("... {x}")`) in `content.rs`, `pipeline.rs`, and `store.rs`.
- Added backticks around `$XDG_CONFIG_HOME/stratum` in CLI arg docs so they
  render as code in `--help`.
- Reverted a `Ratio::get` call in `is_bloated` to use an explicit type ascription
  (`|r: crate::config::Ratio| r.get()`) so it compiles without importing `Ratio`
  into the module scope while keeping the `.map` clear.

## 2026-06-26 — Apply clippy::pedantic hygiene fixes to core and CLI helpers
- Added `Eq` derive to `ConfigSource` so it satisfies contexts that require total
  equality and matches the existing `Eq` derives on `Mode` and `ContentType`.
- Switched `ContentType::ALL` to use `Self::` variants inside the `impl` block,
  removing redundant type repetition.
- Switched `Mode` match arms and equality checks inside `impl Mode` to use `Self::`
  for consistency.
- Updated the `Ratio::try_from` error message to use inline format-capture
  (`{value}`) instead of a separate argument.
- Added backticks around `snake_case` and `ContentType` in doc comments so
  rustdoc renders them as code.
- Verified the standard lint suite still passes.

## 2026-06-26 — Fix float precision in bloat ratio heuristic
- Changed `CompressionContext::bloat_ratio` from `f32` to `f64` so the ratio
  stays exact for inputs larger than ~16 MiB, where `f32` cannot represent every
  byte length.
- Removed the `Ratio` -> `f32` casts in `is_bloated`; thresholds and ratios are
  now compared as `f64` end-to-end, avoiding truncation at the comparison point.
- Updated `DryRunReport::bloat_ratio` to `f64` so the JSON and human dry-run
  reports reflect the same precise value used for offload decisions.
- Added `pipeline::tests::bloat_ratio_stays_precise_for_large_inputs` asserting
  that a 20,000,001-byte input yields exactly `20_000_001.0` with a token budget
  of 1 (and the exact default-budget ratio). Core test count increased from 37
  to 38.

## 2026-06-26 — Harden CI reproducibility and safety margins
- Pinned the `cargo-deny` install in CI to version `0.16.1` with `--locked` so
  supply-chain checks are reproducible and not subject to unexpected breaking
  changes in future `cargo-deny` releases.
- Added `timeout-minutes` to both CI jobs (`check: 20`, `deny: 10`) so a hung
  build or network dependency cannot burn the full default 6-hour GitHub Actions
  quota.
- Updated `README.md` test instructions to match CI: `--all-features` clippy,
  warnings-as-errors build, and rustdoc warnings-as-errors build.

## 2026-06-26 — Add `#[must_use]` to high-value pipeline/report types
- Annotated `CompressionPipeline` with `#[must_use]` so constructing a pipeline
  and never running it produces a compiler warning instead of silently wasted
  work.
- Annotated `DryRunReport` with `#[must_use]` so building a dry-run report and
  never rendering or emitting it also triggers a warning.
- These annotations are small API ergonomics guards that help callers in
  production/enterprise integrations catch accidental drops of constructed
  objects.

## 2026-06-26 — Add unit tests for under-tested CLI helper modules
- Added focused unit tests for `config_source.rs` to guard `ConfigSource::kind`
  and `ConfigSource::to_human` against accidental drift in source labels or
  human-readable descriptions.
- Added focused unit tests for `report.rs` covering `DryRunReport::new`,
  `DryRunReport::to_json`, and `DryRunReport::human`, including:
  - Basic report shape (input length, mode, content type).
  - Offload decision when content is bloated (`Full` mode + tight token budget).
  - `Off` mode suppressing both transforms and offload.
  - JSON and human output containing the expected keys and values.
- `kirkstratum-cli` unit/integration test count increased from 48 to 55.

## 2026-06-26 — Document remaining public CLI helpers for `missing_docs = "deny"`
- Added doc comments to public items in `kirkstratum-cli` helper modules that were
  still bare after the initial documentation pass:
  - `input.rs`: `InputTooLarge`, `InputTooLarge::new`, `max_input_size`,
    `read_input`.
  - `init.rs`: `initialise_config`.
  - `report.rs`: `DryRunReport`, `DryRunReport::new`, `DryRunReport::to_json`,
    `DryRunReport::human`.
- Verified the full suite passes under the stricter lint policy:
  `cargo test --workspace --all-targets`, `cargo test --workspace --doc`,
  `cargo clippy --workspace --all-targets --all-features`,
  `cargo fmt -- --check`,
  `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, and
  `cargo build --workspace --profile ci`.

## 2026-06-26 — Harden CI and lint policy for warnings-as-errors
- Upgraded `missing_docs` from `warn` to `deny` in workspace lints so any new
  undocumented public item fails the build.
- Added `RUSTFLAGS: -D warnings` to the GitHub Actions `check` job so compiler
  warnings are rejected in CI, matching local `RUSTFLAGS="-D warnings"` builds.
- Added `--all-features` to the CI clippy step so feature-gated code is linted
  too.
- Added a `cargo doc --workspace --no-deps` step to CI with
  `RUSTDOCFLAGS: -D warnings` so broken intra-doc links fail the build.
- Fixed the unresolved `[CompressionContext]` link in
  `crates/kirkstratum-core/src/lib.rs` by qualifying it as
  `[CompressionContext](pipeline::CompressionContext)`.

## 2026-06-26 — Remove unused `serde_json` dependency from `kirkstratum-core`
- `serde_json` was declared as a workspace dependency of `kirkstratum-core` but was
  not used by any source file in that crate. Removing it shrinks the core
  dependency tree by 4 crates (`serde_json`, `itoa`, `memchr`, `zmij`), reducing
  supply-chain surface and compile times. The CLI still directly depends on
  `serde_json` for JSON output.

## 2026-06-26 — Expand core unit tests for public API edge cases
- Added tests asserting `Mode::from_str` and `ContentType::from_str` are
  case-insensitive and tolerate leading/trailing whitespace.
- Added tests for `Mode::runs_transforms`, `Mode::offloads_bloat`, and
  `Mode::offload_threshold` so these public helpers cannot silently drift.
- Core test count increased from 32 to 37.

## 2026-06-26 — Add init force and explicit config-dir permission tests
- Added a test asserting `stratum init --force` overwrites an existing config
  file instead of returning an error.
- Extended the explicit `--config-dir` init test to assert `0o600` file
  permissions on Unix, mirroring the existing XDG-path permission check.

## 2026-06-26 — Repository hygiene: untrack build artifacts and Cargo.lock
- Removed `target/` and `Cargo.lock` from git tracking. Both are listed in
  `.gitignore`, but had been committed before the ignore rules were added,
  causing git status noise, large diffs, and CI slowdowns. They remain available
  locally and are still ignored by git.

## 2026-06-26 — Harden config init to create files with restrictive permissions
- `stratum init` now writes `pipeline.toml` with `0o600` permissions on Unix
  so only the owner can read host-specific overrides or tuning values.
- Added a Unix-only test asserting the initialised config file mode is exactly
  `0o600`.
- Windows ACLs remain at the OS default; this follows the common Rust
  cross-platform pattern.

## 2026-06-26 — Harden clap error emission against broken stdout/stderr
- Replaced the panicking `e.print().expect("failed to print clap error")` path
  in `kirkstratum-cli/src/main.rs` with `print_clap_error`, which ignores
  `BrokenPipe` and falls back to a best-effort stderr message for other rare
  I/O failures. This keeps pipelines like `stratum --help | head` from crashing
  the process when the consumer closes the pipe.
- Added a unit test ensuring the clap error print helper does not panic.

## 2026-06-26 — Enforce `missing_docs` lint and document public API
- Added `missing_docs = "warn"` to workspace lints so CI flags
  undocumented public items.
- Documented every remaining public item in `kirkstratum-core`, `kirkstratum-cli`,
  and `kirkstratum-hosts`: enums, structs, fields, constants, methods, traits,
  and `pub mod` declarations.

## 2026-06-26 — Supply-chain surface reduction: remove parking_lot
- Replaced the only workspace use of `parking_lot::RwLock` in
  `InMemoryOffloadStore` with `std::sync::RwLock`, removing `parking_lot`,
  `lock_api`, `parking_lot_core`, `redox_syscall`, and `scopeguard` from the
  dependency tree. This shrinks compile times and reduces supply-chain risk
  while keeping the in-memory store API unchanged.

## 2026-06-26 — Public API docs and dependency cleanup
- Added public doc comments and runnable doc tests for the most common
  `kirkstratum-core` extension points: `ContentType::from_str`, `Mode::from_str`,
  `CompressionContext::with_token_budget`, `PipelineConfig::from_toml`,
  `PipelineConfig::from_file`, `CompressionPipeline::register_content_transform`,
  and `CompressionPipeline::run`.
- Tagged config parse errors with the offending source path and kind so
  `stratum config --validate --json` now includes the file path in the
  machine-readable error output.
- Removed the unused direct `serde_json` dependency from `kirkstratum-hosts`,
  shrinking its dependency surface to only `kirkstratum-core`.

## 2026-06-26 — Reproducible builds and config validation hardening
- Added `rust-toolchain.toml` (channel `1.85.0` with `rustfmt` and `clippy`
  components) so CI, local builds, and contributor environments use the same
  compiler.
- Tightened `cargo-deny` policy: wildcard path dependencies are now denied,
  preventing accidental wildcard imports in a production/enterprise setting.
- Added a drift-guard test in `kirkstratum-core` asserting that
  `PipelineConfig::default()` equals the embedded `config/pipeline.toml`, so
  future edits cannot silently diverge.
- Improved `stratum config --validate --json` so invalid config files emit
  `{ "valid": false, "error": "..." }` while still exiting with code 78.
- Added tests for out-of-range ratio config values and structured JSON error
  output.

## 2026-06-26 — CLI parse-time validation and exit-code consistency
- Added clap value parsers for `--mode`, subcommand mode args, and
  `--content-type` so invalid values are rejected at parse time with the
  supported values listed, and the CLI exits with code 64 (`EX_USAGE`).
- Rejected `0` for `--token-budget` and `--max-input-size` at parse time so
  silently-coerced values cannot surprise operators.
- Mapped `InputTooLarge` via `root_cause()` in the CLI exit-code logic, so
  oversized files that pass the metadata fast-path but fail during bounded
  reading still return code 65 (`EX_DATAERR`) instead of 70 (`EX_SOFTWARE`).
- Changed the unknown-shell `completion` error into a clap usage error so it
  also exits with code 64.
- Added an `#[instrument]` span around `execute_pipeline` for richer trace
  context in production logs.
- Added `ContentTypeParseError` in `kirkstratum-core` so unknown content types list
  the supported values, matching `ModeParseError`.
- Added integration/unit tests for invalid mode/content-type flags, zero-sized
  numeric flags, and `InputTooLarge` exit-code mapping.

## 2026-06-26 — CLI ergonomics and mode error quality
- Added a `--help` smoke test to guard against clap derive regressions.
- Improved `ModeParseError` so unknown modes print the offending value and the
  list of supported modes (`off`, `lite`, `full`, `ultra`).
- Added `Mode` roundtrip and error-message unit tests in `kirkstratum-core`.

## 2026-06-26 — CLI dry-run and rules robustness
- Added `content_type_label` to both human and JSON dry-run reports so script
  consumers and operators see a readable label alongside the snake_case key.
- Extended `kirkstratum-hosts` rules filter tests with edge cases: empty source,
  malformed directives, unclosed markers, repeated directive switches, and
  `Mode::Full` filtering.

## 2026-06-26 — Observability improvements
- Instrumented each content and output transform stage in `CompressionPipeline`
  with a `tracing` span carrying the stage index. Combined with the existing panic
  hook, this makes it easier to map a stage failure or performance anomaly to a
  specific transform.

## 2026-06-26 — Code health and maintainability
- Deduplicated pipeline execution in `kirkstratum-cli/src/main.rs` by extracting an
  `execute_pipeline` helper used by both `run` and `apply`. Future changes to
  config loading, store creation, or dry-run reporting now live in one place.
- Removed the unused `stratum_core::error::TransformError` module and its public
  re-export to eliminate dead surface area.

## 2026-06-26 — Supply-chain and input-safety hardening
- Removed unused workspace dependencies (`memchr`, `sha2`, `rayon`) and the
  unused `sqlite` feature / `rusqlite` optional dependency to shrink the
  supply-chain surface and reduce compile times.
- Hardened `read_input` against oversized files: it now checks file metadata
  before allocating and reads both files and stdin through a shared incremental
  bounded reader, so inputs exceeding `--max-input-size` fail fast instead of
  buffering the entire payload.
- Clarified mode resolution by removing the redundant `STRATUM_MODE` env lookup
  in `mode.rs`; the global `--mode` flag already consumes `STRATUM_MODE` through
  clap's env binding.
- Added focused unit tests for `input` (size limits, UTF-8 errors) and `mode`
  (precedence, invalid values).

## 2026-06-26 — CLI maintainability, observability, and config robustness
- Refactored `kirkstratum-cli` into focused submodules:
  - `cli.rs` — arg definitions and config loading.
  - `config_source.rs` — config provenance enum with human and JSON rendering.
  - `init.rs` — config initialisation.
  - `input.rs` — input reading and size limits.
  - `mode.rs` — mode resolution.
  - `output.rs` — JSON/human emission.
  - `report.rs` — dry-run report.
  - `stdout.rs` — broken-pipe-safe stdout writes.
- Improved `config --sources` JSON output to include both `kind` and
  `description` for every source.
- Improved `ConfigError` messages to clearly identify the offending config file.
- Added a CLI test asserting the TOML dumped by `stratum config` roundtrips
  back to the same effective config.
- Added a global `--mode` flag and `STRATUM_MODE` env var that drives `run`,
  `apply`, `rules`, and `mode` subcommands. Subcommand-specific mode wins over
  the global flag.
- Made all stdout writes resilient to `BrokenPipe` so pipelines like
  `stratum run | head` exit cleanly instead of panicking.
- Added workspace-level lints: `unsafe_code = "forbid"` and `clippy::all = "deny"`,
  wired into every crate via `[lints] workspace = true`.
- Updated CI to build with the `ci` release profile (thin LTO, single codegen unit)
  so release-profile regressions are caught before shipping.
- Fixed config override parsing so explicit `0.0` ratio values are honoured and
  absent fields keep embedded defaults.
- Added regression tests for mode flag precedence, broken-pipe handling, config
  roundtrip, and partial config overrides.

## 2026-06-26 — Config directory flag
- Added a global `--config-dir` flag and `STRATUM_CONFIG_DIR` env var.
- `load_config` now resolves `pipeline.toml` from the configured directory with
  precedence between `--config`, `--config-dir`, XDG, and embedded default.
- `stratum init` writes to `--config-dir` when provided instead of XDG.
- Added `ConfigSource::ConfigDir` and updated `config --sources` output.
- Added regression tests for config-dir precedence, override behaviour, and init.
- Updated `README.md` with `--config-dir` examples and the extended precedence list.

## 2024-06-10 — Initial Analysis
- Created `state.md` and `changelog.md`.
- Initialized git repository and made baseline commit.
- **Correction:** the 8 "failing" tests are not present in source. They represent the ADR acceptance criteria I need to implement.
- Plan: add the missing tests first (TDD), then fix the production code.

## 2026-06-26 — Production/enterprise hardening
- Added `.github/workflows/ci.yml` with fmt/clippy/build/test jobs.
- Renamed `PipelineConfig::from_str` to `from_toml` to satisfy clippy and avoid
  confusion with `std::str::FromStr`.
- Implemented full config precedence chain:
  `STRATUM_CONFIG`/`--config` > `$XDG_CONFIG_HOME/stratum/pipeline.toml` >
  embedded default, backed by an injectable `EnvSource` trait for testing.
- Added regression tests for CLI/XDG/env precedence.
- Structured CLI exit codes: `EX_CONFIG` (78) for config errors, `EX_SOFTWARE`
  (70) for internal errors.
- Initialized `tracing-subscriber` in the CLI with `RUST_LOG`-driven verbosity.
- Fixed placeholder workspace `repository` URL.
- Added `deny.toml` baseline for `cargo deny` license/advisory checks.
- Rewrote `README.md` with build/test/run instructions, config precedence,
  observability, exit codes, and security notes.

## 2026-06-26 — Config initialisation subcommand
- Added `stratum init` subcommand that writes the embedded default config to
  `$XDG_CONFIG_HOME/stratum/pipeline.toml`.
- Refuses to overwrite an existing file unless `--force` is passed.
- Added unit tests for successful initialisation and overwrite protection.
- Documented `stratum init` in `README.md`.

## 2026-06-26 — Dry-run pipeline preview
- Added a global `--dry-run` flag for `run` and `apply`.
- When set, the CLI emits a pipeline plan report instead of transformed output:
  content type, mode, token budget, input length, bloat ratio, and offload
  decision.
- Added a `DryRunReport` struct with human and JSON rendering.
- Added integration tests for `--dry-run` and `--dry-run --json`.
- Documented dry-run usage in `README.md`.

## 2026-06-26 — Shell completion generation
- Added `clap_complete` dependency and a `stratum completion <shell>` subcommand.
- Supports bash, zsh, fish, powershell, and elvish.
- Added integration tests for successful bash completion output and rejection of
  unknown shells.
- Documented completion generation in `README.md`.

## 2026-06-26 — CI supply-chain checks
- Added a `deny` job to `.github/workflows/ci.yml` that installs `cargo-deny` and
  runs `cargo deny check` against `deny.toml` on every push/PR.
- Updated `README.md` to note that CI enforces license/advisory checks.

## 2026-06-26 — Config source visibility
- Added `ConfigSource` enum to track which layers built the effective config:
  `Embedded`, `Xdg`, and `Explicit`.
- Changed `load_config` to return both the merged config and a `Vec<ConfigSource>`.
- Added `stratum config --sources` to print the contributing config files in
  human or `--json` form.
- Added tests asserting the source list contains embedded default and tracks
  explicit/XDG overrides.
- Updated `README.md` with `config --sources` example.

## 2026-06-26 — Observability and panic resilience
- Added `-v`/`--verbose` and `-q`/`--quiet` count flags to control log level at
  runtime (overridable by `RUST_LOG`).
- Configured `tracing-subscriber` to write all logs to stderr so they never mix
  with pipeline output on stdout.
- Added a panic hook that logs the panic location/payload and exits cleanly with
  `EX_SOFTWARE` (70) instead of dumping an unformatted backtrace.
- Added `#[tracing::instrument]` span around the CLI dispatch and `debug!`
  events for config loading and content-type detection.
- Updated `README.md` with `-v`/`-q` examples and the full exit-code table.

## 2026-06-26 — Input limits and structured exit codes
- Added `--max-input-size` global flag and `STRATUM_MAX_INPUT_SIZE` env var,
  defaulting to 50 MiB, for both `run` and `apply`.
- Read stdin incrementally so oversized input fails fast instead of buffering
  the entire payload.
- Added `InputTooLarge` error and mapped it to `EX_DATAERR` (65).
- Mapped `std::io::ErrorKind::NotFound` / `PermissionDenied` to `EX_NOINPUT`
  (66) and clap errors to `EX_USAGE` (64).
- Added integration tests for oversized input and missing file exit codes.
- Documented input limits in `README.md`.

## 2026-06-26 — Machine-readable CLI output
- Added a global `--json` flag per ADR-0016 for scriptable output.
- Wired JSON emission into `config`, `config --validate`, `rules`, `mode`, and
  `version` subcommands.
- Added integration tests asserting JSON keys for each supported subcommand.
- Documented `--json` usage in `README.md`.

## 2026-06-26 — CLI bloat tuning + apply subcommand
- Added `CompressionContext::with_token_budget` so callers can tune the bloat
  heuristic without changing config files.
- Added a global `--token-budget` CLI flag (and `STRATUM_TOKEN_BUDGET` env var)
  that feeds into `run` and `apply`.
- Documented `apply` examples and token-budget usage in `README.md`.

## 2026-06-26 — Pipeline offload + CLI surface expansion
- Wired `PipelineConfig` into `CompressionPipeline::run`.
- Implemented bloat detection with `CompressionContext::is_bloated`, using
  `per_domain` thresholds when present and falling back to the global threshold.
- Added content offload to `OffloadStore` when bloat is detected; replaced
  offloaded content with `[offloaded: <key>]`.
- Added regression tests for offload, no-offload, and per-domain override paths.
- Expanded CLI surface:
  - `rules --mode <mode>` to emit rules for a specific mode.
  - `config` to print the merged effective config as TOML.
  - `config --validate` to validate config and exit with code 78 on errors.
- Added integration tests for `config --validate` success and failure paths.
- Updated `README.md` with new subcommand examples.
