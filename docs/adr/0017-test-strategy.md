# ADR-0017: Test strategy — parity, drift, property, golden

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A plugin that fails silently is worse than a plugin that fails
loudly: the user's agent session produces subtly wrong output,
and the user has no idea why. The test strategy must:

- Catch regressions in the pipeline's compression behaviour.
- Catch drift between the canonical ruleset and the per-host
  adapters.
- Catch drift between the embedded config and the user's
  override.
- Catch property-level invariants (no panic on any input, no
  transform that loses data, etc.).
- Run on every CI build in <5 minutes.

Five test categories cover the surface:

1. **Unit tests** — per-function correctness. Fast, isolated.
2. **Property tests** — invariants across many inputs. Catch
   edge cases that no fixture author thought of.
3. **Golden tests** — fixed input, expected output. Catch
   regression in compression ratios and output shape.
4. **Drift tests** — byte equality between canonical source and
   derived artefacts. Catch divergence between hosts.
5. **Integration tests** — full pipeline run, end-to-end
   through the CLI. Slowest, most realistic.

The five categories together form a pyramid: many unit tests at
the base, fewer property tests above them, fewer golden tests
above those, one or two drift tests at the top, and a single
integration test at the apex.

## Decision

### Unit tests

Every public function in `kirkstratum-core` and `kirkstratum-cli` has
at least one unit test in the same file. The tests assert:

- The function's documented contract.
- Edge cases (empty input, very large input, malformed input).
- Error variants (per ADR-0011's three-variant enum).

A module without a test fails CI:

```yaml
# .github/workflows/ci.yml (sketch)
- name: Coverage gate
  run: cargo llvm-cov --workspace --fail-under-lines 70
```

The 70% line-coverage gate is a floor, not a target. A module
with 100% coverage but no property test still fails the
property-test stage.

### Property tests

`proptest` is a workspace dependency. The key property tests:

1. **No-panic property** — `proptest!` runs every public
   transform with random inputs (uniform bytes, valid JSON,
   valid diffs, mixed content). The transform must return a
   `Result`; it must never panic.

   ```rust
   proptest! {
       #[test]
       fn json_transform_never_panics(s in "\\PC*") {
           let t = JsonReformat::default();
           let _ = t.apply(&s); // never panics
       }
   }
   ```

2. **Idempotence property** — running the pipeline twice on the
   same input produces the same output (modulo offload store
   keys). This is a soft property: offload transforms may
   allocate fresh keys, but the *body* of the output is
   stable.

3. **Bytes-saved property** — `bytes_out <= bytes_in` for
   every transform. A transform that *grows* the input is a
   bug.

4. **Marker validity property** — every `<<stratum:offload:*>>`
   marker in the output is well-formed (24 hex chars, valid
   `validate_key` per ADR-0004).

5. **Mode filtering property** — `build_rules(Mode::Off)`
   returns the empty string. `build_rules(Mode::Full)` returns
   a string that contains every "all" block and every
   "full,ultra" block but no "ultra-only" block.

The no-panic property is the load-bearing one. It catches the
class of bugs that turn "agent session continues with slightly
less compression" into "agent session is dead".

### Golden tests

Golden tests live at `crates/kirkstratum-core/tests/golden/`:

```
crates/kirkstratum-core/tests/golden/
├── json_compact/
│   ├── input.json
│   ├── expected.txt
│   └── metadata.toml       # bloat_threshold, mode, expected bytes_saved
├── log_dedupe/
│   ├── input.log
│   └── expected.txt
├── diff_summary/
│   ├── input.diff
│   └── expected.txt
├── source_collapse/
│   ├── input.rs
│   └── expected.txt
└── pipeline_audit/
    ├── input.txt
    └── expected.toml       # expected StepRecord list
```

A golden test reads `input.*`, runs the pipeline, asserts the
output matches `expected.*` byte-for-byte. The first time a
golden test is added, the author generates the expected output
by running `BLESS=1 cargo test golden`; the expected file is
then committed and reviewed.

```rust
#[test]
fn golden_json_compact() {
    let input = include_str!("golden/json_compact/input.json");
    let expected = include_str!("golden/json_compact/expected.txt");
    let metadata: GoldenMetadata =
        toml::from_str(include_str!("golden/json_compact/metadata.toml")).unwrap();
    let pipeline = build_test_pipeline(&metadata);
    let result = pipeline.run(input, ContentType::JsonObject,
                              &CompressionContext::default(),
                              &InMemoryOffloadStore::new());
    assert_eq!(result.output, expected);
}
```

Golden tests fail loudly when a transform changes behaviour.
The diff in the failing assertion is the load-bearing
diagnostic: a contributor sees exactly which lines of expected
output drifted.

### Drift tests

Three drift tests:

1. **Adapter drift** — every per-host adapter in `kirkstratum-hosts`
   emits the canonical ruleset (ADR-0008) for `Mode::Full`,
   byte-for-byte.

2. **Config drift** — the embedded `pipeline.toml` parses to a
   `PipelineConfig` whose `to_string` matches the source file
   (after normalisation). Catches reformatting accidents.

3. **Skill drift** — the generated `SKILL.md` (ADR-0008 §
   Distribution channel) contains the canonical body for
   `Mode::Full` as a substring.

The adapter drift test is the load-bearing one. It runs on every
CI build and is the single source of truth for "do the per-host
adapters agree?".

### Integration tests

One integration test: `tests/cli_smoke.rs`. It runs:

```bash
echo '{"foo": "bar"}' | cargo run --quiet --bin stratum -- apply
```

And asserts:

- Exit code 0.
- Stdout is non-empty.
- Stderr contains no error messages.
- The output JSON, parsed, has the same keys as the input.

A second integration test exercises the full hook flow:

```bash
echo '{"event": "SessionStart", "context": "startup"}' \
    | cargo run --quiet --bin stratum -- init hook session-start
```

And asserts the output is a JSON envelope that includes the
canonical ruleset for the active mode.

The integration tests are slow (~10 s each) but run on every CI
build.

### Test isolation

Every test that touches the filesystem uses `tempfile::tempdir()`
and cleans up via `Drop`. No test relies on `~/.config/stratum/`
existing. The CLI tests set `STRATUM_CONFIG_DIR`,
`STRATUM_DATA_DIR`, and `STRATUM_RUNTIME_DIR` to tempdir paths
before invoking the binary.

A test that pollutes the host filesystem fails CI.

### Coverage gate

CI runs `cargo llvm-cov --workspace --fail-under-lines 70`. The
70% floor is below the achieved coverage (~85%); it exists to
catch the case where a new module is added without tests.

### What this forbids

- Snapshot tests without a `BLESS=1` regeneration path. A
  golden test whose expected output is hand-edited is a
  snapshot test masquerading as a golden test.
- Property tests that only assert "doesn't panic" without
  also asserting a positive invariant. The no-panic property
  is one of five; a property-test file with one test is not
  enough.
- Integration tests that depend on `cargo run`'s output being
  identical to a release build. The integration tests run the
  debug build and accept that the timing differs.

### What this allows

- Slow tests. A 30-second test that catches a real bug is
  cheaper than a 1-second test that misses it.
- Tests that require network access. There are none in the
  current suite; ADR revision adds one only if a feature
  demands it.
- Mutation testing. `cargo mutants` is a candidate for a
  nightly run; not in CI.

## Consequences

Negative first:

- Five categories of test is one more than three and two more
  than four. A contributor who wants to add a test must pick
  the right category. The rule is: unit first, property when
  invariants apply, golden when bytes matter, drift when
  derived artefacts exist, integration for end-to-end.
- Golden tests require `BLESS=1` discipline. A contributor
  who edits an expected file by hand without re-running the
  pipeline produces a stale expected output. CI catches this
  on the next test run.
- The 70% coverage floor is arbitrary. A future contributor
  may want 80% or 90%. The floor is configurable in CI; the
  decision is structural, not numeric.

Positive:

- The no-panic property is structural: there is no code path
  in the orchestrator that can produce a panic from a
  transform error. The test enforces it.
- Golden tests catch regression in compression ratios. A
  transform that "improves" by reordering its output produces
  a failing golden test, which surfaces the change for review.
- Drift tests catch silent divergence between hosts. A new
  host adapter that forgets to filter by mode fails CI on the
  adapter drift test.

## Implementation notes

The test files live alongside the source:

```
crates/kirkstratum-core/src/store/memory.rs
crates/kirkstratum-core/src/store/memory.rs     # unit tests inline
crates/kirkstratum-core/tests/property.rs        # property tests
crates/kirkstratum-core/tests/golden.rs          # golden test runner
crates/kirkstratum-core/tests/fixtures/          # input/expected pairs

crates/kirkstratum-hosts/src/adapters/*.rs       # unit tests inline
crates/kirkstratum-hosts/tests/copy_drift.rs     # drift test
crates/kirkstratum-hosts/tests/skill_drift.rs    # drift test (generated skill)

crates/kirkstratum-cli/src/hooks/*.rs            # unit tests inline
crates/kirkstratum-cli/tests/cli_smoke.rs        # integration test
```

The `proptest` macros are wrapped in `cfg!(test)` modules to
keep the dev-dependency out of release builds:

```toml
# Cargo.toml workspace
[dev-dependencies]
proptest = "1"
tempfile = "3"
```

A test helper `build_test_pipeline(metadata: &GoldenMetadata) ->
CompressionPipeline` lives at
`crates/kirkstratum-core/src/test_utils.rs` and is `#[cfg(test)]`.

The integration tests use `assert_cmd` for shell-out assertions:

```toml
[dev-dependencies.assert_cmd]
version = "2"
```

```rust
#[test]
fn cli_apply_runs_pipeline() {
    assert_cmd::Command::cargo_bin("stratum")
        .env("STRATUM_CONFIG_DIR", tempfile::tempdir().unwrap().path())
        .env("STRATUM_DATA_DIR", tempfile::tempdir().unwrap().path())
        .env("STRATUM_RUNTIME_DIR", tempfile::tempdir().unwrap().path())
        .write_stdin(r#"{"foo": "bar"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("foo"));
}
```

The CI workflow runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo llvm-cov --workspace --fail-under-lines 70
```

The four steps are the load-bearing CI surface. A PR that
breaks any of them fails the build.