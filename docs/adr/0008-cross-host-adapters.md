# ADR-0008: Cross-host adapter architecture

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

AI agent hosts do not have a unified extension API. Each host has
its own manifest format, its own event names, its own payload
shapes, and its own install conventions. Stratum will support
multiple hosts (Claude Code, Codex, GitHub Copilot CLI, and any
others that adopt the same hook protocol), and the integration
code must:

- Live in one place (`kirkstratum-hosts`) so a single drift test can
  cover every adapter.
- Share a canonical ruleset so the same behaviour injects from every
  adapter, byte-identical.
- Surface host-specific event shapes through a single shim function
  (`emit_to`, ADR-0009) so nothing else in the codebase has to know
  about a host's wire format.
- Fail CI when any per-host adapter drifts from the canonical
  source. Silent drift is the failure mode this ADR exists to
  prevent.

The pattern is "one canonical ruleset + N thin adapter files + one
drift test". The cost of having it is one test file; the cost of
not having it is silent divergence between hosts.

## Decision

### Canonical source

The canonical ruleset lives at `crates/kirkstratum-hosts/docs/rules/CANONICAL.md`. It is
plain markdown with HTML-comment mode directives (ADR-0006):

```markdown
# Stratum — canonical ruleset

This file is the single source of truth. Every per-host adapter
copies the body of this file; the drift test in
`crates/kirkstratum-hosts/tests/copy_drift.rs` enforces byte equality
on the stripped body.

## Core rule: minimum correct change

<!-- stratum:mode:all -->
Ship the smallest change that solves the problem. Three lines that
work beat ten lines that are flexible.

## The ladder

<!-- stratum:mode:full,ultra -->
1. Does this need to exist at all?
2. Stdlib does it? Use it.
3. Native platform feature? Use it.
4. Already-installed dependency solves it? Use it.
5. Can it be one line? One line.
6. Only then: the minimum code that works.

## Worked example: caching

<!-- stratum:mode:ultra -->
User: "Add a cache for these API responses."
Stratum: `lru_cache(maxsize=1000)` on the fetch function. Skipped
a custom cache class, add when lru_cache measurably falls short.
```

### Per-host adapters

Each host family has an adapter directory under
`crates/kirkstratum-hosts/src/adapters/`:

```
crates/kirkstratum-hosts/src/adapters/
├── mod.rs
├── claude_code.rs        # Claude Code, Codex (shared hook format)
├── copilot_cli.rs        # GitHub Copilot CLI (different event names)
├── opencode.rs           # OpenCode (programmatic plugin)
├── pi.rs                 # Pi agent (registerCommand)
└── mcp.rs                # MCP server (escape hatch)
```

Each adapter file is a thin wrapper around two things:

1. The canonical ruleset builder (mode-filtered, ADR-0006).
2. The `emit_to` shim (ADR-0009) that knows the host's event
   shape.

The shared logic — read CANONICAL.md, filter by mode, return the
body — lives in `crates/kirkstratum-hosts/src/rules.rs`:

```rust
use kirkstratum_core::mode::Mode;

const CANONICAL: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/rules/CANONICAL.md"));

pub fn build_rules(mode: Mode) -> String {
    let mut out = String::with_capacity(CANONICAL.len());
    let mut keep = true;
    for line in CANONICAL.lines() {
        if let Some(rest) = line.strip_prefix("<!-- stratum:mode:") {
            let directive = rest.strip_suffix("-->").unwrap_or(rest).trim();
            keep = matches!(directive, "all")
                || directive.split(',').any(|m| m.trim() == mode.as_str());
            continue;
        }
        if keep { out.push_str(line); out.push('\n'); }
    }
    out
}

/// Fallback when CANONICAL.md is missing from the build (e.g. an
/// installed binary whose docs were stripped). The fallback is
/// hard-coded so adapters still emit *something* coherent.
pub fn fallback_rules(mode: Mode) -> String {
    match mode {
        Mode::Off => String::new(),
        Mode::Lite => "Stratum: lite mode. Prefer the smallest correct change.\n".into(),
        Mode::Full => "Stratum: full mode. The ladder applies.\n".into(),
        Mode::Ultra => "Stratum: ultra mode. The ladder applies. Worked examples enabled.\n".into(),
    }
}
```

The `fallback_rules` is the load-bearing safety net. An adapter
that fails to read CANONICAL.md (it shouldn't, because
`include_str!` is compile-time, but the safety net exists for
`crates/kirkstratum-hosts/docs/rules/CANONICAL.md` being empty or stripped) returns a short
fallback string so the user sees *something* rather than silence.

### Drift test

The drift test lives at
`crates/kirkstratum-hosts/tests/copy_drift.rs`:

```rust
use kirkstratum_hosts::adapters;
use kirkstratum_hosts::rules::{build_rules, CANONICAL};

#[test]
fn canonical_body_present() {
    assert!(!CANONICAL.is_empty(), "CANONICAL.md is empty");
}

#[test]
fn every_mode_directive_is_well_formed() {
    for (i, line) in CANONICAL.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("<!-- stratum:mode:") {
            let directive = rest.strip_suffix("-->").unwrap_or(rest).trim();
            assert!(
                directive == "all"
                    || directive.split(',').all(|m| matches!(m.trim(),
                        "off" | "lite" | "full" | "ultra"))
                    || directive.split(',').all(|m| matches!(m.trim(),
                        "off" | "lite" | "full" | "ultra" | "all")),
                "line {}: malformed directive {:?}", i + 1, directive
            );
        }
    }
}

#[test]
fn every_adapter_emits_canonical_body_for_mode_full() {
    let canonical_filtered = build_rules(kirkstratum_core::mode::Mode::Full);
    for adapter in adapters::all() {
        let emitted = adapter.emit_rules(kirkstratum_core::mode::Mode::Full);
        assert_eq!(
            emitted, canonical_filtered,
            "adapter {} drifted from canonical",
            adapter.name()
        );
    }
}

#[test]
fn every_adapter_handles_unknown_event_gracefully() {
    for adapter in adapters::all() {
        let result = adapter.handle_event("__nonexistent_event__", &serde_json::json!({}));
        assert!(result.is_ok(), "adapter {} panicked on unknown event", adapter.name());
    }
}
```

The `adapters::all()` function returns a `Vec<&'static dyn Adapter>`.
Each adapter implements:

```rust
pub trait Adapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn emit_rules(&self, mode: Mode) -> String;
    fn handle_event(&self, event: &str, payload: &serde_json::Value)
        -> Result<serde_json::Value, AdapterError>;
}
```

`handle_event` calls `emit_to` (ADR-0009) and is the *only* place
that knows about host event shapes.

### Distribution channel: portable skill packages

Some hosts (and downstream marketplaces) consume a portable skill
format. We generate a derived `docs/rules/skills/stratum/SKILL.md`
with a single-line YAML frontmatter from the canonical body. The
generator is a build script:

```rust
// crates/kirkstratum-hosts/build.rs
fn main() {
    println!("cargo:rerun-if-changed=docs/rules/CANONICAL.md");
    let canonical = std::fs::read_to_string("docs/rules/CANONICAL.md")
        .expect("CANONICAL.md");
    let pkg = format!(
        "---\nname: stratum\ndescription: Pipeline + behaviour layer for AI agent context.\nlicense: MIT\n---\n\n{}",
        canonical
    );
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap())
        .join("SKILL.md");
    std::fs::write(out, pkg).expect("write SKILL.md");
}
```

The drift test asserts the generated `SKILL.md` body matches the
canonical body for `Mode::Full`. This is the same drift test as
above, just with a different consumer.

### Per-host instruction-only fallback

For hosts that only support project instructions (no hooks, no
plugins), the user copies `crates/kirkstratum-hosts/docs/rules/CANONICAL.md` into the host's
rules directory (`AGENTS.md`, `.cursor/rules/stratum.mdc`, etc.).
The drift test does not cover this case — the user is responsible
for the copy. The README documents the path for each supported
host.

## Consequences

Negative first:

- Every per-host adapter that wants to integrate must implement
  the `Adapter` trait, register in `adapters::all()`, and pass the
  drift test. A one-line typo in `emit_rules` fails CI. This is the
  intended friction.
- The drift test compares *bytes* for the `Full` mode. Any change
  to CANONICAL.md that adds whitespace, fixes a typo, or reorders
  a list forces every adapter author to re-read and re-run the
  test. This is the cost of byte-stability.
- The portable skill generator is a build script. Build scripts
  are notoriously easy to break; the drift test is the safety net.

Positive:

- One canonical source. Editing CANONICAL.md and running `cargo
  test` shows every drift in seconds.
- New host adapters are short. A new host that reuses the Claude
  Code event shape is ~20 lines of Rust.
- The fallback rules ensure adapters never silently emit empty
  strings. The user sees *something*, even if the canonical file
  is missing.

## Implementation notes

The `Adapter` trait lives in `crates/kirkstratum-hosts/src/adapter.rs`.
The implementations live in
`crates/kirkstratum-hosts/src/adapters/<host>.rs`. The `all()` function
lives in `crates/kirkstratum-hosts/src/adapters/mod.rs`:

```rust
pub fn all() -> Vec<&'static dyn super::Adapter> {
    vec![
        &claude_code::ClaudeCodeAdapter,
        &copilot_cli::CopilotCliAdapter,
        &opencode::OpenCodeAdapter,
        &pi::PiAdapter,
        &mcp::McpAdapter,
    ]
}
```

The `stratum_mode:Mode` type re-exported from `kirkstratum-core` is the
single `Mode` enum (ADR-0006). The `kirkstratum-hosts` crate depends
on `kirkstratum-core` and `serde_json` and nothing else from the host
SDKs.

The build script path is `crates/kirkstratum-hosts/build.rs`. Its
output is consumed by `src/skill.rs` via `include_str!` on the
generated path:

```rust
pub const SKILL_MD: &str = include_str!(concat!(env!("OUT_DIR"), "/SKILL.md"));
```

The drift test for the generated skill is a separate test:

```rust
#[test]
fn generated_skill_matches_canonical_for_full_mode() {
    use kirkstratum_hosts::skill::SKILL_MD;
    let canonical = build_rules(Mode::Full);
    let skill_body = SKILL_MD
        .splitn(3, "---").nth(2).unwrap_or("").trim();
    assert!(skill_body.contains(&canonical),
        "generated SKILL.md body missing canonical content");
}
```

This is a substring match rather than a byte-equality match because
the frontmatter is added by the generator. The substring assertion
ensures the canonical body is present without forcing an exact
byte match on the frontmatter.
