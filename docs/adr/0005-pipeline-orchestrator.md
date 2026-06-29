# ADR-0005: Pipeline orchestrator with parallel bloat estimation

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A pipeline that runs every registered transform serially on every
input is both slow and wrong. It is slow because most inputs do not
benefit from most transforms. It is wrong because running a
low-confidence offload when a high-confidence reformat has already
packed the content is wasted work.

The orchestrator must:

1. Filter transforms by `applies_to` against the detected content
   type.
2. Run the cheap `estimate_bloat` for every eligible offload in
   parallel, so a single large input pays one scan, not N.
3. Run reformats serially (they may depend on each other's output)
   until one of them stops saving bytes.
4. Decide which offloads are worth running based on their
   `estimate_bloat` score and the configured bloat threshold.
5. Run selected offloads serially, in registration order.
6. Never panic on a transform error (ADR-0011). Every failure is a
   skip with a tracing event.
7. Produce a `PipelineResult` that names every step it took, so the
   CLI can print a one-line summary and tests can assert on the
   exact step sequence.

## Decision

The orchestrator lives at
`crates/kirkstratum-core/src/pipeline/orchestrator.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use rayon::prelude::*;

use crate::content::ContentType;
use crate::context::CompressionContext;
use crate::error::TransformError;
use crate::store::OffloadStore;
use super::traits::{OffloadOutput, OffloadTransform, ReformatOutput, ReformatTransform};

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub reformat_target_ratio: f32,    // stop reformats when savings < ratio
    pub bloat_threshold: f32,          // offloads above this score run
    pub offload_fallback_ratio: f32,   // if no offload wins, accept this ratio of input
    pub per_domain: HashMap<ContentType, DomainOverrides>,
}

impl PipelineConfig {
    pub const DEFAULT: &'static str = include_str!("../../config/pipeline.toml");
    pub fn from_str(s: &str) -> Result<Self, ConfigError> { /* ... */ }
    pub fn from_file(p: &Path) -> Result<Self, ConfigError> { /* ... */ }
    pub fn merge(&mut self, override_cfg: &PipelineConfig);  // see ADR-0007
}

#[derive(Debug, Clone, Default)]
pub struct DomainOverrides {
    pub bloat_threshold: Option<f32>,
    pub reformat_target_ratio: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub output: String,
    pub bytes_in: usize,
    pub bytes_out: usize,
    pub steps_applied: Vec<StepRecord>,
    pub cache_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StepRecord {
    pub transform: &'static str,
    pub kind: StepKind,
    pub outcome: StepOutcome,
    pub bytes_saved: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind { Reformat, Offload }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome { Applied, Skipped, Errored }

pub struct CompressionPipeline {
    reformats_by_type: HashMap<ContentType, Vec<Arc<dyn ReformatTransform>>>,
    offloads_by_type: HashMap<ContentType, Vec<Arc<dyn OffloadTransform>>>,
    config: PipelineConfig,
}

impl CompressionPipeline {
    pub fn builder() -> CompressionPipelineBuilder { CompressionPipelineBuilder::default() }

    pub fn run(
        &self,
        content: &str,
        content_type: ContentType,
        ctx: &CompressionContext,
        store: &dyn OffloadStore,
    ) -> PipelineResult {
        // Implementation below.
    }
}
```

### `CompressionPipelineBuilder`

```rust
#[derive(Default)]
pub struct CompressionPipelineBuilder {
    reformats: Vec<Arc<dyn ReformatTransform>>,
    offloads: Vec<Arc<dyn OffloadTransform>>,
    config: Option<PipelineConfig>,
}

impl CompressionPipelineBuilder {
    pub fn register_reformat(mut self, t: Arc<dyn ReformatTransform>) -> Self {
        self.reformats.push(t); self
    }
    pub fn register_offload(mut self, t: Arc<dyn OffloadTransform>) -> Self {
        self.offloads.push(t); self
    }
    pub fn with_config(mut self, c: PipelineConfig) -> Self {
        self.config = Some(c); self
    }
    pub fn build(self) -> CompressionPipeline {
        let config = self.config.unwrap_or_default();
        let mut reformats_by_type: HashMap<_, Vec<_>> = HashMap::new();
        for r in self.reformats {
            for ct in r.applies_to() {
                reformats_by_type.entry(*ct).or_default().push(r.clone());
            }
        }
        let mut offloads_by_type: HashMap<_, Vec<_>> = HashMap::new();
        for o in self.offloads {
            for ct in o.applies_to() {
                offloads_by_type.entry(*ct).or_default().push(o.clone());
            }
        }
        CompressionPipeline { reformats_by_type, offloads_by_type, config }
    }
}
```

### Decision flow

```rust
pub fn run(&self, content: &str, ct: ContentType, ctx: &CompressionContext,
           store: &dyn OffloadStore) -> PipelineResult {
    let bytes_in = content.len();
    let mut current = content.to_string();
    let mut steps = Vec::new();
    let mut keys = Vec::new();

    // Phase 1: parallel bloat estimation across every eligible offload.
    let offloads = self.offloads_by_type.get(&ct).map(Vec::as_slice).unwrap_or(&[]);
    let scored: Vec<(usize, f32)> = offloads.par_iter().enumerate()
        .map(|(i, o)| (i, o.estimate_bloat(&current)))
        .collect();

    // Phase 2: serial reformats. Each reformat operates on the previous
    // output. Stop when the ratio of savings falls below target_ratio.
    let reformats = self.reformats_by_type.get(&ct).map(Vec::as_slice).unwrap_or(&[]);
    for r in reformats {
        match r.apply(&current) {
            Ok(out) => {
                let saved = current.len().saturating_sub(out.output.len());
                let ratio = saved as f32 / current.len().max(1) as f32;
                steps.push(StepRecord {
                    transform: r.name(), kind: StepKind::Reformat,
                    outcome: StepOutcome::Applied, bytes_saved: saved,
                });
                current = out.output;
                if ratio < self.config.reformat_target_ratio { break; }
            }
            Err(TransformError::Skipped) => {
                steps.push(StepRecord {
                    transform: r.name(), kind: StepKind::Reformat,
                    outcome: StepOutcome::Skipped, bytes_saved: 0,
                });
            }
            Err(e) => {
                tracing::warn!(transform = r.name(), error = %e, "reformat errored");
                steps.push(StepRecord {
                    transform: r.name(), kind: StepKind::Reformat,
                    outcome: StepOutcome::Errored, bytes_saved: 0,
                });
            }
        }
    }

    // Phase 3: gated offloads. Run in registration order those whose
    // bloat estimate exceeds the threshold (or the per-domain override).
    let threshold = self.config.per_domain.get(&ct)
        .and_then(|d| d.bloat_threshold)
        .unwrap_or(self.config.bloat_threshold);
    let mut ran_any_offload = false;
    for (i, o) in offloads.iter().enumerate() {
        let score = scored.iter().find(|(j, _)| *j == i).map(|(_, s)| *s).unwrap_or(0.0);
        if score < threshold && ran_any_offload { continue; }
        ran_any_offload = true;
        match o.apply(&current, ctx, store) {
            Ok(out) => {
                steps.push(StepRecord {
                    transform: o.name(), kind: StepKind::Offload,
                    outcome: StepOutcome::Applied, bytes_saved: out.bytes_saved,
                });
                keys.push(out.cache_key);
                current = out.output;
            }
            Err(TransformError::Skipped) => {
                steps.push(StepRecord {
                    transform: o.name(), kind: StepKind::Offload,
                    outcome: StepOutcome::Skipped, bytes_saved: 0,
                });
            }
            Err(e) => {
                tracing::warn!(transform = o.name(), error = %e, "offload errored");
                steps.push(StepRecord {
                    transform: o.name(), kind: StepKind::Offload,
                    outcome: StepOutcome::Errored, bytes_saved: 0,
                });
            }
        }
    }

    PipelineResult {
        bytes_out: current.len(),
        output: current,
        bytes_in,
        steps_applied: steps,
        cache_keys: keys,
    }
}
```

### Why parallel bloat estimation

`estimate_bloat` is the orchestrator's gating signal. If it is
sequential, a content type with N registered offloads pays N scans
of the input. For a 1 MB log dump, that is the difference between
one pass and N. `rayon::join` (or `par_iter`) over a small `Vec`
gives a near-linear speedup until the per-thread overhead dominates
(at ~8 threads it is roughly flat, so we do not oversubscribe).

`estimate_bloat` must be allocation-free (ADR-0003) for this to be
safe; if it allocates, the parallel scan blows the memory budget.

### Why serial reformats

Reformats may depend on each other's output: a JSON whitespace
collapser and a JSON key deduplicator both consume JSON, but the
deduplicator is cheaper on collapsed input. Running them serially
in registration order lets each one see the savings of the previous.
The early-exit on `reformat_target_ratio` prevents infinite chains
of micro-savings.

### Why a `ran_any_offload` flag

If no offload passes the bloat threshold, we do not run any of them.
If at least one passes, we run it and any subsequent offloads in
registration order regardless of their individual scores. The flag
prevents a single high-scoring offload from starving a lower-scoring
but still-useful one.

The fallback ratio handles the "no offload won" case: if no offload
ran, the orchestrator emits a tracing event at `info` level with
the highest score seen and the threshold used. Tests assert on this
event.

## Consequences

Negative first:

- `par_iter` introduces a rayon dependency in core. ADR-0002 already
  listed rayon as allowed; this ADR ratifies the choice.
- The "run all offloads after the first one passes" policy is
  opinionated. A user who wants to run *only* the highest-scoring
  offload gets neither that nor a clean way to opt in. If that
  becomes a real need, ADR revision adds an `OffloadSelection`
  enum to `PipelineConfig`.
- The orchestrator does not currently support cancel-on-error. A
  transform that returns `Internal` is logged and skipped, but the
  pipeline continues. This is the ADR-0011 policy at work; if a
  transform author needs fail-fast, they return `Internal` from
  `apply` and the user reads the `StepOutcome::Errored` records.

Positive:

- The orchestrator's decision flow is one function: `run`. There
  is no state machine, no actor, no event loop. Tests can call
  `run` directly with synthetic transforms and assert on
  `steps_applied`.
- `PipelineResult` carries the full audit trail. The CLI prints
  a one-line summary (`"in=1234 out=456 saved=778 (3 reformat, 1 offload)"`)
  and tests assert on the exact step list.
- The parallel estimation is the load-bearing performance trick.
  It is invisible from the trait surface (transform authors do not
  need to know about rayon), so the abstraction holds.

## Implementation notes

`PipelineConfig::DEFAULT` is the embedded TOML string (ADR-0007).
The `Default` impl is:

```rust
impl Default for PipelineConfig {
    fn default() -> Self {
        Self::from_str(Self::DEFAULT).expect("embedded config must parse")
    }
}
```

The `expect` is acceptable because the embedded string is
compile-time constant and is exercised by a unit test that calls
`PipelineConfig::from_str(Self::DEFAULT)` and asserts on a few
key fields. If the embedded config fails to parse, the binary
fails to build.

Tests live in `crates/kirkstratum-core/src/pipeline/orchestrator.rs`
under `#[cfg(test)] mod tests`. The minimum test set:

1. `reformat_chain_terminates_on_target_ratio` — register three
   reformats with progressively smaller savings; assert that the
   third is skipped.
2. `offload_below_threshold_is_skipped` — register one offload
   with `estimate_bloat` returning 0.1; assert it is not in
   `steps_applied`.
3. `erroring_transform_is_logged_and_continues` — register a
   transform that returns `TransformError::Internal`; assert
   the result is `StepOutcome::Errored` and the pipeline
   continues.
4. `parallel_estimation_runs_for_every_offload` — register five
   offloads; assert all five `estimate_bloat` calls were made
   even when only one passes the threshold.
5. `result_audit_trail_names_every_step` — run a fixed input;
   assert `steps_applied` matches a golden list.

The drift / parity tests for the orchestrator live in
`crates/kirkstratum-core/tests/pipeline_golden.rs` and use the
fixtures under `examples/fixtures/` (ADR-0017).
