# ADR-0003: The two-transform split (Reformat vs Offload)

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

The engine's job is to shrink content. There are two fundamentally
different ways to shrink content, and conflating them is the single
most common architectural mistake in this problem space:

1. **Reformat**: the content stays semantically intact but is packed
   more densely. Examples: collapsing JSON whitespace, deduplicating
   repeated keys in a JSON array, abbreviating log timestamps, joining
   adjacent blank lines, normalising line endings, dropping trailing
   whitespace. The result is *lossless* — the original is recoverable
   by formatting. No external store is needed.
2. **Offload**: bytes are dropped, and a *marker* replaces them. The
   full original is stashed in an `OffloadStore` (ADR-0004) under a
   key, and the marker embeds that key so an agent (or a follow-up
   tool call) can retrieve the original on demand. The result is
   *lossy from the wire's perspective* but recoverable through the
   store. Examples: truncating huge stack traces, dropping low-relevance
   log lines, collapsing repeated sub-trees in deeply nested JSON.

Treating these as one thing leads to transforms that quietly drop
bytes without a retrievable key (silent data loss) or transforms that
shuffle bytes without ever reducing them (false advertising). The
type system should make both classes of bug impossible.

## Decision

We define two traits in `crates/kirkstratum-core/src/pipeline/traits.rs`:

```rust
use crate::content::ContentType;
use crate::error::TransformError;
use crate::store::OffloadStore;
use crate::context::CompressionContext;

/// A lossless reformat. Produces semantically equivalent output,
/// bytes are recoverable by reformatting the input. No external
/// store is consulted.
pub trait ReformatTransform: Send + Sync {
    fn name(&self) -> &'static str;

    /// Content types this transform is willing to attempt.
    /// The orchestrator filters; the transform must still handle
    /// unexpected input by returning `TransformError::Skipped`.
    fn applies_to(&self) -> &[ContentType];

    fn apply(&self, content: &str) -> Result<ReformatOutput, TransformError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformatOutput {
    pub output: String,
    pub bytes_saved: usize,
}

/// A lossy offload. Drops bytes and emits a marker; the original
/// is in the store under `cache_key`. The cache key is REQUIRED —
/// the type system enforces this.
pub trait OffloadTransform: Send + Sync {
    fn name(&self) -> &'static str;

    fn applies_to(&self) -> &[ContentType];

    /// Cheap structural signal, must be safe on empty input, must
    /// not allocate. Runs in parallel for every registered offload
    /// (ADR-0005).
    fn estimate_bloat(&self, content: &str) -> f32;

    fn apply(
        &self,
        content: &str,
        ctx: &CompressionContext,
        store: &dyn OffloadStore,
    ) -> Result<OffloadOutput, TransformError>;

    /// Confidence in [0, 1]; used as a tie-breaker when multiple
    /// offloads both pass their bloat threshold.
    fn confidence(&self) -> f32 { 0.5 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffloadOutput {
    pub output: String,
    pub bytes_saved: usize,
    /// REQUIRED. The marker emitted in `output` references this key.
    /// A transform that drops bytes without producing a retrievable
    /// key must return `TransformError::InvalidInput` instead.
    pub cache_key: String,
}
```

The orchestrator (ADR-0005) owns the decision flow: which transforms
are eligible, in what order they run, what happens on failure. The
traits above own only the *per-content* contract.

### Content type

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    JsonArray,
    JsonObject,
    SourceCode,
    SearchResults,
    BuildOutput,
    GitDiff,
    Html,
    PlainText,
}
```

`ContentType` is `Copy + Eq + Hash` because it is the index key for
the orchestrator's transform registry. Detection of `ContentType` is
covered in ADR-0014.

### Compression context

```rust
#[derive(Debug, Clone, Default)]
pub struct CompressionContext {
    /// The user query, if known. Offloads may use this for relevance
    /// scoring (e.g. keep lines that mention the query).
    pub query: Option<String>,
    /// Optional token budget; transforms may use it as a hint.
    pub token_budget: Option<usize>,
}
```

The context is propagated only to offloads; reformats do not need it
because they preserve semantics.

### Marker format

Offload markers are emitted in the output as:

```
<<stratum:offload:{cache_key}>>
```

The exact format is owned by `kirkstratum-core` (in
`src/pipeline/marker.rs`). The `OffloadOutput::cache_key` is the
hash that follows the prefix. Retrieval is a separate trait method
on `OffloadStore`:

```rust
pub trait OffloadStore: Send + Sync {
    fn put(&self, payload: &str) -> String;
    fn get(&self, key: &str) -> Option<String>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }
}
```

ADR-0004 covers the `put` returning the key (the trait is responsible
for key generation) and the backend selection policy.

### "Reformat" vs "Offload" naming

We deliberately avoid the words "lossless" and "lossy". Reformat is
lossless by construction (no store consulted); offload is lossy on
the wire but recoverable through the store. The naming tracks
*what the transform does*, not *what the operator might fear*.

## Consequences

Negative first:

- A transform that is sometimes lossless and sometimes lossy cannot
  fit either trait. Such transforms must be split into two. This is
  a feature, not a bug: it forces the author to declare which mode
  they are in.
- A transform that needs an external store *and* preserves semantics
  does not exist in this model. The intended use case is "always
  offload, then reformat the marker-bearing output"; if a single
  pass is desired, write two transforms and let the orchestrator
  chain them.
- `OffloadOutput::cache_key` is `String`, not `Option<String>`. A
  transform author cannot return success without a key. Returning
  `Err(TransformError::InvalidInput)` is the explicit "I tried but
  cannot produce a marker" path.

Positive:

- The orchestrator's contract is uniform: every registered transform
  is either a reformat (no store) or an offload (store required).
  No "is this transform enabled? does it need a store?" branching.
- A transform author who reads the trait sees the entire contract on
  one screen. There are no hidden side effects, no implicit ordering,
  no surprise `&mut self` requirement.
- Tests for a single transform need only the trait plus the store
  trait (if offload). No orchestrator, no config, no event loop.

## Implementation notes

The two traits live in `crates/kirkstratum-core/src/pipeline/traits.rs`.
The output types live in the same file. The marker format lives in
`crates/kirkstratum-core/src/pipeline/marker.rs` and exposes:

```rust
pub fn render(key: &str) -> String;
pub fn parse(marker: &str) -> Option<&str>;     // returns the key
pub const PREFIX: &str = "<<stratum:offload:";
pub const SUFFIX: &str = ">>";
```

`estimate_bloat` is the most-constrained method. It must:

- Return a value in `[0.0, 1.0]` where `0.0` is "definitely not bloated"
  and `1.0` is "definitely bloated".
- Be allocation-free. Use `memchr::count` or a fixed-size counter.
- Be safe on empty input (return `0.0`).
- Be domain-specific: a log transform knows "lots of duplicate
  timestamps"; a JSON transform knows "lots of repeated keys".

A transform author who writes an `estimate_bloat` that allocates will
get an immediate clippy lint from a custom rule (`clippy.toml`):

```toml
disallowed-methods = [
    { path = "std::string::String::with_capacity", reason = "estimate_bloat must not allocate" },
    { path = "std::vec::Vec::with_capacity", reason = "estimate_bloat must not allocate" },
    { path = "format!", reason = "estimate_bloat must not allocate" },
]
```

This is a structural lint. The build fails on the first commit that
violates it.
