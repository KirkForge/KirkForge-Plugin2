# ADR-0001: Plugin purpose and identity

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

We are building a Rust plugin that will live in `KirkForge-Plugin2` and
integrate with multiple AI agent hosts (Claude Code, Codex, GitHub Copilot
CLI, and others that adopt the same hook protocol). The plugin needs a
clear identity so that contributors and downstream tools can reason about
its scope. Without a sharp identity, scope creep is the default outcome.

Two prior implementations informed the design:

- One was a context-compression library that shrank tool outputs and logs
  before they reached an LLM, exposing both a Rust engine and a thin HTTP
  proxy. Its strongest contribution was the typed `Reformat` vs `Offload`
  split and the loud-failure backend factory.
- The other was an always-on system-prompt skill that biased an LLM
  toward the smallest correct change, with a Mode enum (off/lite/full/
  ultra) and a canonical-ruleset-plus-per-host-adapter architecture. Its
  strongest contribution was the single-source-of-truth + drift-test
  pattern and the `ponytail:` deferral comment convention.

Neither codebase is referenced by name anywhere in the source or in these
ADRs. The ideas they contain are the inheritance; the names are not.

## Decision

The plugin is named **Stratum** (crate `stratum`, binary `stratum`,
marketplace id `stratum@local`). It does exactly one thing and refuses
to grow beyond it:

> **Stratum is a pipeline that transforms content destined for an AI
> agent's context window — packing dense outputs, offloading bulky
> content to a local store with retrievable markers, and applying
> agent-facing rules of behaviour — all driven by a four-level Mode
> enum and exposed through a cross-host hook layer.**

Concretely, Stratum owns three responsibilities:

1. **A pluggable transformation pipeline.** Content entering the
   pipeline is classified by type (JSON, source code, logs, diffs,
   search results, plain text), then run through any number of
   registered transforms. Two transform kinds exist (ADR-0003):
   *reformat* (lossless pack, no retrieval) and *offload* (lossy drop
   with a marker that can be retrieved). A pluggable store (ADR-0004)
   holds offloaded originals.
2. **A mode-driven behaviour layer.** A `Mode { Off, Lite, Full, Ultra }`
   enum (ADR-0006) selects which transforms run, how aggressive they
   are, and which rules of agent behaviour are injected. `Off` is a
   real variant, not the absence of a value (ADR-0012).
3. **A cross-host hook integration layer.** A canonical ruleset +
   per-host adapter files + a drift test (ADR-0008) keeps the plugin
   portable across agent hosts. A single per-host output shim
   (ADR-0009) owns every difference in event shape.

Stratum explicitly does **not**:

- Make network requests. No telemetry, no auto-update, no phone-home.
- Run as a long-lived daemon by default. The binary is CLI-first; the
  only stateful surface is a hook shim that exits per-event.
- Depend on a machine-learning runtime. Content detection (ADR-0014)
  uses byte signatures and structural sniffing, not ONNX/magika.
- Maintain an install database. State lives in XDG directories
  (ADR-0015) with a single config file and a single flag file.
- Provide a UI. The CLI is the interface; humans who want pretty
  output pipe through `jq` or similar.

## Consequences

Negative first, as is the convention:

- Anyone who wants Stratum to "phone home for updates" or "auto-tune
  thresholds from a backend" will be disappointed. The scope is fixed.
- The mode enum is opinionated. Users who want five or ten intensity
  levels must add them through ADR revision; we will not silently
  extend the enum.
- Pluggable transforms are an architectural commitment. Every new
  transform type must implement a trait, register at startup, and pass
  the drift / parity tests (ADR-0017).

Positive:

- The "one thing" promise makes the codebase teachable. A new
  contributor can read ADRs 0001, 0003, 0004, and 0005 and understand
  the entire engine.
- Scoping out the daemon, the ML runtime, and the install DB
  eliminates three large sources of complexity that historically
  bloat CLI plugins.
- The mode-driven behaviour layer means the same binary can be
  installed everywhere from a CI runner to a developer's laptop,
  with different intensity per installation.

## Implementation notes

The plugin's identity is reflected in three places, and only three:

1. `Cargo.toml` `[package]` — `name = "stratum"`,
   `description = "Pipeline + behaviour layer for AI agent context"`.
2. The marketplace manifest at `.claude-plugin/marketplace.json`
   declares `stratum@local`.
3. The binary name (`stratum`) and the crate root (`src/lib.rs` +
   `src/main.rs`) — the binary is the user-facing identity.

There is no separate "About" page, no tagline in the README beyond
the one-sentence description above, and no marketing copy in source
files. ADRs are the documentation surface.
