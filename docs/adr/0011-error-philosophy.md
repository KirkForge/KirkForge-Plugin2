# ADR-0011: Error philosophy: three variants, never panic

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A transform author who returns `Result::Err(_)` is making a
statement: "I could not do my job; please skip me and continue."
A transform author who panics is making a different statement:
"I have encountered a state I cannot recover from." In a pipeline
that runs user-supplied content through arbitrary code, the
difference between these two statements is the difference between
"the agent's session continues with slightly less compression"
and "the agent's session is dead".

The orchestrator (ADR-0005) must never panic on a transform
error. Every failure mode must be expressed as a value, and the
value must fit into one of a small number of variants. Three
variants is the right number:

1. **Invalid input** — the transform recognised the content as
   its kind but the content was malformed (e.g. JSON that starts
   with `{` but is not actually parseable).
2. **Skipped** — the transform decided not to act on this content
   (e.g. estimate_bloat returned below threshold; the content is
   not the kind the transform handles).
3. **Internal** — the transform hit an unexpected error (e.g.
   the offload store returned `None` for a key the transform just
   inserted; an I/O error; a logic bug).

Every variant is a "skip and continue". The orchestrator logs the
error and moves on. The pipeline result records the step with the
right `StepOutcome` (ADR-0005).

## Decision

The error type lives at `crates/stratum-core/src/error.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformError {
    /// The input was malformed for this transform's domain.
    /// Example: a JSON transform received text that looks like
    /// JSON but does not parse. The orchestrator records this as
    /// StepOutcome::Errored and moves on.
    InvalidInput { reason: String },

    /// The transform chose not to act on this content. Not an
    /// error; a normal outcome. The orchestrator records this as
    /// StepOutcome::Skipped and moves on.
    Skipped { reason: String },

    /// The transform hit an unexpected error. The orchestrator
    /// records this as StepOutcome::Errored, logs at warn level,
    /// and moves on.
    Internal { message: String },
}

impl TransformError {
    pub fn invalid<S: Into<String>>(reason: S) -> Self {
        Self::InvalidInput { reason: reason.into() }
    }
    pub fn skipped<S: Into<String>>(reason: S) -> Self {
        Self::Skipped { reason: reason.into() }
    }
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::Internal { message: message.into() }
    }
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { reason } => write!(f, "invalid input: {}", reason),
            Self::Skipped { reason } => write!(f, "skipped: {}", reason),
            Self::Internal { message } => write!(f, "internal error: {}", message),
        }
    }
}

impl std::error::Error for TransformError {}
```

### Orchestrator handling

The orchestrator's `run` method (ADR-0005) handles each variant
exactly once, at the top of each `match`:

```rust
match r.apply(&current) {
    Ok(out) => { /* ... */ }
    Err(TransformError::Skipped { reason }) => {
        tracing::debug!(transform = r.name(), reason = %reason, "skipped");
        steps.push(StepRecord {
            transform: r.name(), kind: StepKind::Reformat,
            outcome: StepOutcome::Skipped, bytes_saved: 0,
        });
    }
    Err(TransformError::InvalidInput { reason }) => {
        tracing::warn!(transform = r.name(), reason = %reason, "invalid input");
        steps.push(StepRecord {
            transform: r.name(), kind: StepKind::Reformat,
            outcome: StepOutcome::Errored, bytes_saved: 0,
        });
    }
    Err(TransformError::Internal { message }) => {
        tracing::error!(transform = r.name(), message = %message, "internal error");
        steps.push(StepRecord {
            transform: r.name(), kind: StepKind::Reformat,
            outcome: StepOutcome::Errored, bytes_saved: 0,
        });
    }
}
```

The pipeline continues regardless of which variant was returned.
There is no `?` propagation inside the orchestrator's loop.

### What this forbids

A transform author must not:

- Call `panic!`, `unwrap`, `expect`, or `unreachable!` in
  `apply` or `estimate_bloat`. A custom clippy lint catches this:

  ```toml
  # clippy.toml
  disallowed-methods = [
      { path = "std::panic", reason = "transforms must return Result, never panic" },
  ]
  ```

  The clippy lint is enforced in CI for the `stratum-core` crate.

- Return `Result::Err` with a custom error type. The trait says
  `Result<_, TransformError>`; an author who wants richer errors
  must wrap them in `TransformError::Internal`.

- Silently swallow errors with `.ok()`. A transform that does not
  know how to handle a `None` should return
  `TransformError::Internal`.

### What this allows

A transform author may:

- Return `Ok(output)` even when `output.output == input`. A
  no-op transform is a valid outcome (the orchestrator records
  `bytes_saved: 0` and moves on).
- Return `Err(TransformError::Skipped)` for any reason. The
  `reason` field is a human-readable string used in `tracing::debug!`.
- Return `Err(TransformError::InvalidInput)` when the input is
  malformed. This is the right variant for "the user's JSON is
  not actually JSON".

### Offload store errors

The `OffloadStore::put` and `get` methods (ADR-0004) return
`Result` internally; the trait surface is infallible
(`fn put(&self, payload: &str) -> String`). A store that fails to
write logs the error at `error` level and returns a fresh,
random key whose `get` will return `None`. The transform sees a
working store and proceeds; the agent's retrieval later will
fail gracefully (the marker is in the output but the original is
not in the store).

This is a deliberate trade-off: a transform author never has to
handle store errors. The store is responsible for surfacing its
own failures through logs.

## Consequences

Negative first:

- "Never panic" is a strong invariant that requires a custom
  clippy lint and a code-review discipline to enforce. A
  contributor who adds `unwrap()` to a transform passes CI until
  the lint is enabled in their environment; the review is the
  safety net.
- Three variants is one more than two and one less than four.
  A future contributor who wants `Timeout` or `Cancelled` will
  be tempted to add a variant. The answer is "wrap in
  `Internal`" — keep the variant count fixed.
- The offload store's silent-on-failure behaviour is a sharp
  edge. A misconfigured store will produce markers that cannot
  be retrieved. The drift test asserts that the store logs at
  `error` level on every failure, which is the only signal an
  operator has.

Positive:

- The orchestrator's `run` method is straightforward. There is no
  error-propagation logic in the hot path; every error is a
  step record.
- A user who sees `StepOutcome::Errored` in the pipeline result
  knows exactly which transform failed and why. The CLI prints
  a one-line summary.
- Tests for transforms are simple: assert the output or assert
  the error variant. There is no error-type hierarchy to navigate.

## Implementation notes

The `TransformError` enum lives in
`crates/stratum-core/src/error.rs`. The orchestrator's match
arms live in `crates/stratum-core/src/pipeline/orchestrator.rs`
(ADR-0005). The clippy lint lives at the workspace root in
`clippy.toml`.

Tests for the error philosophy:

1. `transform_returning_skipped_is_logged_at_debug` — register
   a transform that returns `Skipped`; assert no warn/error logs.
2. `transform_returning_invalid_is_logged_at_warn` — register a
   transform that returns `InvalidInput`; assert a warn log.
3. `transform_returning_internal_is_logged_at_error` — register
   a transform that returns `Internal`; assert an error log.
4. `pipeline_continues_after_transform_error` — register three
   transforms, the second returns `Internal`; assert all three
   appear in `steps_applied` and the pipeline produces output.
5. `clippy_lint_catches_unwrap_in_transform` — a fixture file
   under `tests/ui/` with `.unwrap()` in a transform; assert
   `cargo clippy --tests -- -D warnings` fails.

Test 5 requires the `compiletest` suite. It is set up under
`crates/stratum-core/tests/ui/` with one fixture file,
`unwrap_in_transform.rs`. CI runs `cargo clippy --workspace
--all-targets -- -D warnings`, which is sufficient to catch
violations without the compiletest suite. The fixture exists for
documentation purposes.

The store's silent-on-failure behaviour is tested in
`crates/stratum-core/src/store/sqlite.rs`:

```rust
#[test]
fn put_failure_logs_and_returns_random_key() {
    let dir = tempfile::tempdir().unwrap();
    let read_only_path = dir.path().join("readonly.db");
    std::fs::write(&read_only_path, b"").unwrap();
    let mut perms = std::fs::metadata(&read_only_path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&read_only_path, perms).unwrap();
    let store = SqliteOffloadStore::open(&read_only_path, 60).unwrap();
    let key = store.put("hello");
    assert_eq!(key.len(), 24);
    assert!(store.get(&key).is_none());
}
```

This is the only place we use `unwrap()` in a test; it is
explicit and documented.
