# ADR-0009: Per-host output shim

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

Each AI agent host has its own event wire format. A few examples:

- **Claude Code** `SessionStart` expects raw stdout that is the
  system-prompt addition (or JSON with `hookSpecificOutput.additionalContext`).
- **Codex** `SessionStart` expects JSON with `systemMessage` and
  optionally `hookSpecificOutput`.
- **GitHub Copilot CLI** uses different event names (`sessionStart`,
  `userPromptSubmitted`) and a different payload shape.
- **OpenCode** does not consume hooks at all; it consumes a server
  plugin that returns a `transform` function applied to the
  outgoing system prompt.
- **Pi agent** consumes an extension that calls
  `pi.registerCommand` and listens to lifecycle events.
- **MCP** exposes rules as a `prompt` and a `tool` over stdio JSON-RPC.

If this knowledge is scattered through the codebase, every change to
a host's event format is a multi-file edit. If it is centralised in
one function, every change is a one-function edit. The function is
the load-bearing abstraction.

The same shim handles:

- Rules injection (the canonical ruleset, mode-filtered).
- Mode-change echo (when a user runs `/stratum off`, the shim
  returns a JSON payload that tells the host to update its status
  bar).
- Error envelopes (a transform that failed in a hook handler must
  return a JSON envelope the host can parse).

## Decision

The shim is `crates/kirkstratum-hosts/src/shim.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use kirkstratum_core::mode::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Host {
    ClaudeCode,
    Codex,
    CopilotCli,
    OpenCode,
    Pi,
    Mcp,
}

impl Host {
    pub fn from_env(env: &dyn EnvSource) -> Self {
        // Order of detection: explicit env > well-known vars.
        if let Some(s) = env.get("STRATUM_HOST") {
            if let Ok(h) = s.parse() { return h; }
        }
        if env.get("CLAUDE_CODE").is_some() { return Host::ClaudeCode; }
        if env.get("CODEX_HOME").is_some() { return Host::Codex; }
        if env.get("COPILOT_PLUGIN_DATA").is_some() { return Host::CopilotCli; }
        if env.get("OPENCODE").is_some() { return Host::OpenCode; }
        if env.get("PI_AGENT").is_some() { return Host::Pi; }
        Host::Mcp // last resort default; the MCP server explicitly sets STRATUM_HOST=mcp
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    SessionStart,
    UserPromptSubmit,
    SubagentStart,
    PreToolUse,
}

#[derive(Debug, Clone)]
pub enum Payload {
    InjectRules { mode: Mode, rules: String },
    ModeChange { new_mode: Mode, old_mode: Mode },
    Ack,
    Error { message: String },
}

/// Single source of truth for host event shapes.
/// Every adapter calls this function; nothing else in the codebase
/// knows about a host's wire format.
pub fn emit_to(host: Host, event: Event, payload: Payload) -> Value {
    match host {
        Host::ClaudeCode => claude_code_emit(event, payload),
        Host::Codex => codex_emit(event, payload),
        Host::CopilotCli => copilot_emit(event, payload),
        Host::OpenCode => opencode_emit(event, payload),
        Host::Pi => pi_emit(event, payload),
        Host::Mcp => mcp_emit(event, payload),
    }
}

// Each per-host emitter is a small match on (event, payload).
// None of them are more than ~25 lines. The single source of truth
// means a new host is one new function in this file.

fn claude_code_emit(event: Event, payload: Payload) -> Value {
    match (event, payload) {
        (Event::SessionStart, Payload::InjectRules { rules, .. }) => {
            // Claude Code reads raw stdout for SessionStart, or JSON
            // with hookSpecificOutput for newer versions. Emit both
            // shapes for forward-compat: stdout is the body, JSON
            // envelope is also acceptable.
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": rules,
                },
                "systemMessage": rules,
            })
        }
        (Event::UserPromptSubmit, Payload::ModeChange { new_mode, old_mode }) => {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": format!(
                        "stratum: mode {} -> {}", old_mode, new_mode
                    ),
                },
            })
        }
        (Event::SubagentStart, Payload::InjectRules { rules, .. }) => {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "SubagentStart",
                    "additionalContext": rules,
                },
            })
        }
        _ => json!({}),
    }
}

fn codex_emit(event: Event, payload: Payload) -> Value {
    // Codex uses the same event names as Claude Code but a different
    // envelope: systemMessage + hookSpecificOutput are siblings, not
    // nested. The handler chain is otherwise identical.
    match (event, payload) {
        (Event::SessionStart, Payload::InjectRules { rules, .. }) => json!({
            "systemMessage": rules,
            "hookSpecificOutput": { "additionalContext": rules },
        }),
        (Event::UserPromptSubmit, Payload::ModeChange { new_mode, old_mode }) => json!({
            "systemMessage": format!("stratum: mode {} -> {}", old_mode, new_mode),
        }),
        _ => json!({}),
    }
}

fn copilot_emit(event: Event, payload: Payload) -> Value {
    // Copilot CLI uses different event names and a flatter envelope.
    match (event, payload) {
        (Event::SessionStart, Payload::InjectRules { rules, .. }) => json!({
            "additionalContext": rules,
        }),
        (Event::UserPromptSubmit, Payload::ModeChange { new_mode, old_mode }) => json!({
            "additionalContext": format!("stratum: mode {} -> {}", old_mode, new_mode),
        }),
        _ => json!({}),
    }
}

fn opencode_emit(event: Event, payload: Payload) -> Value {
    // OpenCode is a server plugin, not a hook. The "payload" is
    // applied as a transform on the outgoing system prompt.
    match (event, payload) {
        (Event::SessionStart, Payload::InjectRules { rules, .. }) => json!({
            "config": {},
            "experimental.chat.system.transform": rules,
        }),
        _ => json!({}),
    }
}

fn pi_emit(event: Event, payload: Payload) -> Value {
    // Pi agent registers commands and listens to lifecycle events.
    // The "payload" is what pi.before_agent_start receives.
    match (event, payload) {
        (Event::SessionStart, Payload::InjectRules { rules, .. }) => json!({
            "system_prompt_suffix": rules,
        }),
        _ => json!({}),
    }
}

fn mcp_emit(event: Event, payload: Payload) -> Value {
    // MCP is the escape hatch: the payload becomes a tool response.
    match (event, payload) {
        (Event::SessionStart, Payload::InjectRules { rules, .. }) => json!({
            "content": [{ "type": "text", "text": rules }],
        }),
        _ => json!({}),
    }
}
```

### Detection

`Host::from_env` is the *only* place that reads host env vars.
The detection order is:

1. Explicit `STRATUM_HOST` env var (always wins).
2. Well-known host env vars (`CLAUDE_CODE`, `CODEX_HOME`, etc.).
3. Default to `Host::Mcp` (the most permissive shape).

A test asserts the detection order:

```rust
#[test]
fn stratum_host_env_wins_over_well_known_vars() {
    let env = MockEnv::new()
        .set("STRATUM_HOST", "claude_code")
        .set("CLAUDE_CODE", "1")
        .set("CODEX_HOME", "/codex");
    assert_eq!(Host::from_env(&env), Host::ClaudeCode);
}
```

### Unknown event handling

If a host emits an event that `emit_to` does not handle, the
function returns `json!({})` (an empty JSON object). The drift
test (ADR-0008) asserts that no adapter panics on unknown events.

## Consequences

Negative first:

- A new host requires adding a new variant to `Host` and a new
  `*_emit` function. There is no plugin mechanism for new hosts;
  ADR revision is required. This is intentional — the shim is
  the place where event shapes are owned, and every new host is a
  deliberate design decision.
- The `claude_code_emit` function emits *both* the legacy stdout
  shape and the new JSON envelope, for forward compatibility. A
  host that strictly enforces one shape will see the other as
  noise. The README documents this dual-emit behaviour.
- `Host::from_env` defaults to `Mcp`. A user who runs the binary
  in an unknown environment will silently behave as an MCP server.
  This is the safest default (MCP is the most permissive shape),
  but it is also the most surprising. The README troubleshooting
  section lists the `STRATUM_HOST` env var as the override.

Positive:

- Every change to a host's event format is a one-function edit in
  `shim.rs`. Nothing else in the codebase needs to change.
- The drift test (ADR-0008) covers every host through the `Adapter`
  trait. A new host that breaks an event shape fails CI on the
  next test run.
- The detection logic is centralised and tested. A user who sets
  `STRATUM_HOST=codex` to debug a Codex-specific issue gets
  Codex-specific behaviour without rebuilding.

## Implementation notes

The `Host` enum and `emit_to` function live at
`crates/kirkstratum-hosts/src/shim.rs`. The `Adapter` trait
(ADR-0008) calls `emit_to` from its `handle_event` method:

```rust
impl Adapter for ClaudeCodeAdapter {
    fn name(&self) -> &'static str { "claude-code" }
    fn emit_rules(&self, mode: Mode) -> String {
        build_rules(mode)
    }
    fn handle_event(&self, event: &str, payload: &Value) -> Result<Value, AdapterError> {
        let parsed_event = parse_event(event)?;
        let parsed_payload = parse_payload(payload)?;
        Ok(emit_to(Host::ClaudeCode, parsed_event, parsed_payload))
    }
}
```

`parse_event` and `parse_payload` are also in `shim.rs`:

```rust
pub fn parse_event(s: &str) -> Result<Event, AdapterError> {
    match s {
        "SessionStart" | "sessionStart" => Ok(Event::SessionStart),
        "UserPromptSubmit" | "userPromptSubmitted" => Ok(Event::UserPromptSubmit),
        "SubagentStart" | "agent_start" => Ok(Event::SubagentStart),
        "PreToolUse" => Ok(Event::PreToolUse),
        _ => Err(AdapterError::UnknownEvent(s.to_string())),
    }
}
```

The `AdapterError` is `#[derive(thiserror::Error)]`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("unknown event: {0}")]
    UnknownEvent(String),
    #[error("invalid payload: {0}")]
    InvalidPayload(String),
}
```

The drift test asserts that no adapter turns `AdapterError` into a
panic. The `handle_event` method returns `Err`; the host's caller
decides whether to surface the error or swallow it.
