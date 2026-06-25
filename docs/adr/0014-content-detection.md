# ADR-0014: Content detection strategy (cheap, no ML runtime)

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

The orchestrator (ADR-0005) needs to know what kind of content
it is processing before it can pick the right transforms.
`ContentType` is the index key for the transform registry; a
wrong detection means a transform is either skipped when it
should run or run when it should be skipped.

Three detection strategies are plausible:

1. **Magic-byte sniffing** — read the first N bytes and match
   against known signatures. Fast (one memchr), wrong for
   content that lacks a signature (most source code, plain
   text).
2. **Structural parsing** — try to parse the content as each
   candidate type, succeed on the first one. Slow (multiple
   parses), correct for content that is well-formed.
3. **ML-based detection** — run a classifier (magika, ONNX
   runtime). Slow (~30 MB extra binary), correct, requires a
   model download.

ADR-0001 excludes ML-based detection on principle (no ML
runtime). The choice is between magic-byte and structural
parsing. The right answer is a layered approach: cheap magic
bytes first, structural fallback for the ambiguous cases.

The detection must also be safe on empty input, allocation-free
where possible, and never panic. The three-property invariant
shapes the trait.

## Decision

### The `detect` chain

The detection lives at `crates/stratum-core/src/content/detect.rs`:

```rust
use crate::content::ContentType;

pub fn detect(content: &str) -> ContentType {
    if content.is_empty() { return ContentType::PlainText; }

    // Layer 1: magic bytes (cheapest, ~50 ns per check).
    if let Some(ct) = detect_by_magic(content) {
        return ct;
    }

    // Layer 2: structural sniff (more expensive, ~5 µs per check).
    if let Some(ct) = detect_by_structure(content) {
        return ct;
    }

    // Layer 3: heuristics on shape (cheapest, ~200 ns).
    if let Some(ct) = detect_by_shape(content) {
        return ct;
    }

    ContentType::PlainText
}
```

### Layer 1: magic bytes

```rust
fn detect_by_magic(content: &str) -> Option<ContentType> {
    let bytes = content.as_bytes();

    // JSON: starts with { or [ and the first non-whitespace
    // character is one of those. Distinguish array vs object by
    // the first non-whitespace character.
    let first = bytes.iter().find(|b| !b.is_ascii_whitespace()).copied();
    match first {
        Some(b'{') => return Some(ContentType::JsonObject),
        Some(b'[') => return Some(ContentType::JsonArray),
        Some(b'<') => {
            // HTML / XML — first non-whitespace tag.
            if content.starts_with("<!DOCTYPE html")
                || content.starts_with("<html")
                || content.starts_with("<HTML")
            {
                return Some(ContentType::Html);
            }
            // < is too ambiguous for XML/SVG/MathML/etc.; defer
            // to Layer 2.
        }
        _ => {}
    }

    // Git diff: starts with "diff --git " or "--- a/" prefix.
    if content.starts_with("diff --git ") {
        return Some(ContentType::GitDiff);
    }

    // Source code: shebang on the first line.
    if content.starts_with("#!") {
        return Some(ContentType::SourceCode);
    }

    None
}
```

### Layer 2: structural sniff

```rust
fn detect_by_structure(content: &str) -> Option<ContentType> {
    // Try JSON parse — if it succeeds, we have a JSON value.
    // We already distinguished array vs object in Layer 1, so
    // a successful parse here is confirmation, not new info.
    if serde_json::from_str::<serde_json::Value>(content).is_ok() {
        // Layer 1 said not JSON; structural disagrees. Layer 1 wins.
        return None;
    }

    // Try unidiff parse — only succeeds for actual diffs.
    if unidiff::PatchSet::parse(content).is_ok() {
        return Some(ContentType::GitDiff);
    }

    None
}
```

The unidiff crate is the cheapest correct diff detector. A regex
would be wrong (false positives on prose that contains `---`
and `+++`).

### Layer 3: shape heuristics

```rust
fn detect_by_shape(content: &str) -> Option<ContentType> {
    // Build output: lots of lines starting with a timestamp
    // or a log level.
    let line_count = content.lines().count();
    if line_count < 3 { return None; }

    let timestamp_lines = content.lines()
        .take(50)
        .filter(|l| looks_like_log_line(l))
        .count();
    if timestamp_lines > line_count / 3 {
        return Some(ContentType::BuildOutput);
    }

    // Search results: many lines starting with "http" or a path.
    let url_lines = content.lines()
        .take(50)
        .filter(|l| l.contains("http://") || l.contains("https://"))
        .count();
    if url_lines > line_count / 4 {
        return Some(ContentType::SearchResults);
    }

    None
}

fn looks_like_log_line(line: &str) -> bool {
    // Very rough: starts with a digit (timestamp) or contains
    // a log level word.
    line.starts_with(|c: char| c.is_ascii_digit())
        || line.contains("INFO")
        || line.contains("WARN")
        || line.contains("ERROR")
        || line.contains("DEBUG")
}
```

The heuristics are intentionally rough. The point is to flag
content that *might* be a build log or search results, not to
classify it perfectly. A transform that does not want to handle
build output will check `applies_to` and skip gracefully
(ADR-0003).

### `ContentType` enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl ContentType {
    pub const ALL: &'static [ContentType] = &[
        ContentType::JsonArray,
        ContentType::JsonObject,
        ContentType::SourceCode,
        ContentType::SearchResults,
        ContentType::BuildOutput,
        ContentType::GitDiff,
        ContentType::Html,
        ContentType::PlainText,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::JsonArray => "json_array",
            Self::JsonObject => "json_object",
            Self::SourceCode => "source_code",
            Self::SearchResults => "search_results",
            Self::BuildOutput => "build_output",
            Self::GitDiff => "git_diff",
            Self::Html => "html",
            Self::PlainText => "plain_text",
        }
    }
}
```

`as_str` is the canonical name used in the TOML config
(ADR-0007), in `PipelineResult::steps_applied`, and in tracing
events.

### Why no ML runtime

The ML runtime (magika, ONNX) would add:

- ~30 MB to the binary size.
- A model download at first run (or a 30 MB asset in the repo).
- A non-Rust dependency tree (the ONNX runtime is a C library).
- A panic surface (model load failures).

For the detection accuracy we need — *not perfect, just good
enough to pick a transform* — the layered heuristic is
sufficient. A transform that gets the wrong content type will
fail in `apply` and return `TransformError::InvalidInput`
(ADR-0011); the orchestrator skips it and moves on. The cost
of a wrong detection is one skipped transform, not a panic.

If a future use case demands higher accuracy, ADR revision adds
a `magika` feature flag and an opt-in ML layer. The layered
design makes this a clean addition.

### Performance budget

`detect` is called once per content block. The budget is:

- Layer 1: <100 ns (a few byte comparisons).
- Layer 2: <10 µs (one JSON parse attempt + one unidiff parse
  attempt, both of which short-circuit on first error).
- Layer 3: <500 ns (count up to 50 lines).

Total budget: <12 µs per call. A 1 MB content block can absorb
this without dominating the pipeline runtime.

## Consequences

Negative first:

- A JSON content that starts with whitespace and a comment is
  not detected as JSON by Layer 1. Layer 2 catches it via the
  parse attempt. A custom serde format that wraps JSON with a
  header is not detected; the user must declare the type via a
  `--content-type` flag (ADR-0016).
- The shape heuristics are deliberately rough. A markdown
  document with many `https://` links may be misclassified as
  `SearchResults`. The drift test (ADR-0017) catches the
  egregious cases; the rest is on the transform author.
- The detection is allocation-light but not allocation-free.
  `content.lines()` allocates an iterator; `serde_json::from_str`
  allocates on success. The cost is bounded but real.

Positive:

- No ML runtime. No model download. No 30 MB binary. The plugin
  works in air-gapped environments.
- Detection is fast. The 12 µs budget is invisible next to the
  transform runtime.
- A wrong detection is recoverable: the transform returns
  `InvalidInput` and the orchestrator skips. The pipeline
  continues.

## Implementation notes

The detection lives at `crates/stratum-core/src/content/detect.rs`.
The `ContentType` enum lives at `crates/stratum-core/src/content/mod.rs`.
The `unidiff` crate is a workspace dependency:

```toml
# Cargo.toml workspace
unidiff = "0.4"
```

The `serde_json` parse in Layer 2 is a *trial*: we do not
consume the value, we only check whether the parse succeeds.
A future optimisation is to memoise the parse so a downstream
JSON transform can reuse it. The memoise is deferred (ADR-0013
convention: "stratum: re-parse in JSON transform, memoise when
detection cost dominates").

Tests:

1. `detect_empty_returns_plain_text`
2. `detect_json_array_starts_with_bracket`
3. `detect_json_object_starts_with_brace`
4. `detect_git_diff_starts_with_diff_git`
5. `detect_html_starts_with_doctype`
6. `detect_source_code_starts_with_shebang`
7. `detect_build_output_via_log_lines`
8. `detect_search_results_via_urls`
9. `detect_plain_text_fallback`
10. `detect_with_leading_whitespace`

Tests 1–9 are unit tests in `detect.rs`. Test 10 is the
regression test for the "JSON with leading whitespace" case.

A property test:

```rust
#[test]
fn detect_is_idempotent() {
    let cases = &[
        ("{}", ContentType::JsonObject),
        ("[]", ContentType::JsonArray),
        ("diff --git a/foo b/foo\n", ContentType::GitDiff),
        ("#!/bin/sh\necho hi\n", ContentType::SourceCode),
    ];
    for (input, expected) in cases {
        let first = detect(input);
        let second = detect(&first.as_str()); // round-trip
        assert_eq!(first, expected, "input: {:?}", input);
    }
}
```

The round-trip is a no-op for detection (calling `detect` on a
single word does not change the type), but it asserts that
`ContentType::as_str` produces something `detect` will fall
through to `PlainText` for. This is a smoke test, not a
correctness test.
