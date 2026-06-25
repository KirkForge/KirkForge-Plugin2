# ADR-0012: Whole-message deactivation match (string-match lessons)

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A user can disable Stratum with a slash command (`/stratum off`)
or with a natural-language phrase. The natural-language path is
where the design must be careful: substring matching for a
deactivation phrase is a footgun.

Concretely, the failed experiment was:

```rust
// BAD: substring match
fn is_deactivation(text: &str) -> bool {
    text.to_lowercase().contains("stop ponytail") // historical bug
        || text.to_lowercase().contains("normal mode")
}
```

The phrase "add a normal mode toggle" matched the second branch
and turned Stratum off mid-task. The user was asking for a
*feature* (a toggle in the UI) and the matcher interpreted it as
a *command* (turn Stratum off). The user's session continued
without Stratum's rules, and the agent produced output that did
not match the user's expectations.

The fix is structural, not heuristic: the deactivation phrase
must match the *whole input message*, not a substring. This is
the rule every contributor must internalise: any phrase that
mutates global plugin state must be matched against the entire
input, not against substrings.

The same lesson applies to mode-change commands. A user who types
"can you put it in lite mode for me" is *not* issuing a
`/stratum lite` command; they are asking the agent to do
something. The agent should answer the question, not silently
switch modes.

## Decision

### The deactivation matcher

```rust
// crates/stratum-hosts/src/command.rs

/// Returns true iff `text` is a whole-message deactivation phrase.
/// Whole-message means: trimmed, lowercased, exact equality against
/// one of the canonical phrases. Substring matching is forbidden
/// here (see ADR-0012 for the incident).
pub fn is_deactivation(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    let stripped = normalized.trim_end_matches(|c: char| {
        c.is_ascii_punctuation() || c.is_whitespace()
    });
    matches!(stripped, "stop stratum" | "normal mode")
}
```

The two canonical phrases are `stop stratum` and `normal mode`.
A user who wants to disable Stratum types one of these as their
*entire message*. Anything else — including a sentence that
contains either phrase — does not deactivate.

### The slash command parser

The `/stratum <mode>` parser is the *only* path that changes
mode mid-session. It requires the literal `/stratum` prefix:

```rust
pub fn parse_command(text: &str, current: Mode) -> StratumCommand {
    let t = text.trim();
    let body = match t.strip_prefix("/stratum") {
        Some(rest) => rest.trim(),
        None => return StratumCommand::NotACommand,
    };
    if body.is_empty() { return StratumCommand::Status; }
    if let Ok(mode) = body.parse::<Mode>() {
        return StratumCommand::SetMode(mode);
    }
    if body.eq_ignore_ascii_case("default") {
        return StratumCommand::SetDefault(current);
    }
    StratumCommand::Invalid(body.to_string())
}
```

`StratumCommand::NotACommand` is the explicit "this is not a
command" case. It is distinct from `Invalid` (which means "this
looks like a command but is malformed").

### What this forbids

A contributor must not add any of these patterns:

- `text.contains("stratum")` for any state-changing purpose.
- `text.matches("mode")` as a trigger.
- A regex that matches a deactivation phrase anywhere in the
  input.
- A "smart" matcher that uses an LLM to decide whether the user
  meant a command.

The custom clippy lint catches `contains` on `str` inside
`stratum-hosts`:

```toml
# clippy.toml (workspace root)
disallowed-methods = [
    { path = "str::str::Str::contains", reason = "use whole-message match for state changes (ADR-0012)" },
]
```

The lint is too broad to enable blanket; instead, the rule is
documented in the contributor guide and enforced by review. (A
future ADR may add a scoped `clippy::pedantic` group for
`stratum-hosts` that catches `contains` in `command.rs`.)

### What this allows

- Whole-message equality match (this ADR).
- Slash command prefix match (`/stratum ...`).
- Trimmed, lowercased, punctuation-stripped comparison for
  *display* purposes only (e.g. rendering the mode badge).

### "Off" is a real mode

`Mode::Off` is a value in the `Mode` enum (ADR-0006). It is not
the absence of a mode. The status bar reflects it
(`[STRATUM:OFF]`), the rules filter strips everything, the
orchestrator skips every transform. A user who wants Stratum
*silent* (no rules, no transforms, no status bar) can disable
both the rules and the status bar separately:

- `STRATUM_NUDGE_STATUSLINE=0` (ADR-0010) silences the nudge.
- `Mode::Off` silences rules and transforms.
- There is no "completely silent" mode; the status bar is the
  last word on whether Stratum is present.

This is a deliberate non-feature: a user who has gone to the
trouble of running `/stratum off` deserves visible confirmation
that Stratum is off.

## Consequences

Negative first:

- A user who wants a keyboard shortcut for "off" must type one
  of the two canonical phrases exactly. Anything else does not
  work. This is the cost of the safety property.
- The `is_deactivation` function has only two canonical phrases.
  A user who prefers "disable stratum" or "turn off stratum"
  must use `/stratum off`. The README documents the canonical
  phrases.
- Whole-message match means a user who types
  "Stop stratum. By the way, also..."
  does not deactivate Stratum (the trailing text breaks the
  whole-message equality). This is correct: the user wrote a
  multi-sentence message, and only the first sentence is
  canonical.

Positive:

- The substring-match bug is structurally impossible. There is
  no code path that uses substring matching for state changes.
- The slash command prefix is unambiguous. The agent knows
  whether the user typed a command or asked a question.
- `Mode::Off` is observable. The status bar, the rules filter,
  and the orchestrator all handle it explicitly. A `match mode`
  that forgets `Off` fails to compile.

## Implementation notes

The matcher lives at `crates/stratum-hosts/src/command.rs`. Tests
in the same file:

1. `whole_message_match_disables`
   - `"stop stratum"` → `true`
   - `"STOP STRATUM"` → `true`
   - `"stop stratum."` → `true` (trailing punctuation stripped)
   - `"stop stratum!"` → `true`
   - `"please stop stratum"` → `false`
   - `"can you add a stop stratum button"` → `false`
   - `"add a normal mode toggle"` → `false` (the original incident)

2. `slash_command_recognised`
   - `"/stratum"` → `Status`
   - `"/stratum off"` → `SetMode(Off)`
   - `"/stratum lite"` → `SetMode(Lite)`
   - `"/stratum default"` → `SetDefault(current)`
   - `"/stratum bogus"` → `Invalid("bogus".into())`
   - `"please /stratum off"` → `NotACommand`
   - `"stratum off"` (no slash) → `NotACommand`

3. `mode_off_strips_every_rule`
   - `build_rules(Mode::Off)` → empty string.
4. `mode_off_skips_every_transform`
   - `CompressionPipelineBuilder::register_reformat(...).build().run(...)` with
     `Mode::Off` configured → `steps_applied` is empty.

Test 1's `"can you add a stop stratum button"` case is the
regression test for the original bug. It is named in a comment so
the next contributor who tries to "improve" the matcher sees the
incident report.
