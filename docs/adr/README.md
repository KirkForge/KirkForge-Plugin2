# Stratum — Architecture Decision Records

This directory contains the architecture decisions for **Stratum**, a Rust
plugin that transforms content flowing into AI agent contexts. The decisions
in these ADRs are the source of truth for how the codebase is built. An LLM
reading these ADRs in order should be able to scaffold the workspace,
implement every trait, and ship a working binary without further context.

## Index

| #     | Title                                                   | Status   |
|-------|---------------------------------------------------------|----------|
| 0001  | Plugin purpose and identity                             | Accepted |
| 0002  | Workspace layout and crate boundaries                   | Accepted |
| 0003  | The two-transform split (Reformat vs Offload)           | Accepted |
| 0004  | OffloadStore trait and backend selection (loud failure) | Accepted |
| 0005  | Pipeline orchestrator with parallel bloat estimation    | Accepted |
| 0006  | Mode enum and mode-filtered configuration               | Accepted |
| 0007  | TOML defaults embedded via `include_str!`               | Accepted |
| 0008  | Cross-host adapter architecture                         | Accepted |
| 0009  | Per-host output shim (single event-shape source)        | Accepted |
| 0010  | Hooks integration model for AI agent hosts              | Accepted |
| 0011  | Error philosophy: three variants, never panic           | Accepted |
| 0012  | Whole-message deactivation match (string-match lessons) | Accepted |
| 0013  | The `stratum:` comment convention for deferred work     | Accepted |
| 0014  | Content detection strategy (cheap, no ML runtime)       | Accepted |
| 0015  | State management (XDG config dir, flag file, no DB)     | Accepted |
| 0016  | CLI design: clap-derive, env override precedence        | Accepted |
| 0017  | Test strategy: parity, drift, property, golden          | Accepted |
| 0018  | Build profile and feature gating discipline             | Accepted |

## Reading order

Read in numerical order for first-time implementers. ADRs 0003–0006 form
the core engine; 0008–0010 form the integration layer; the rest are
supporting decisions. Cross-references between ADRs are explicit.

## Format

Each ADR follows the Michael Nygard layout:

1. **Status** — `Accepted`, `Proposed`, `Superseded by ADR-NNNN`.
2. **Context** — the problem being solved, with the forces in play.
3. **Decision** — what we will do. Includes Rust types, traits, file paths
   where they clarify the decision.
4. **Consequences** — the trade-offs, both positive and negative. Lazy
   developers always list the negative side first.
5. **Implementation notes** — concrete file paths, function signatures,
   and snippets an implementer can paste.

The `ponytail:`/`stratum:` convention (ADR-0013) is used inline in code
samples to mark deferred simplifications and their ceilings.
