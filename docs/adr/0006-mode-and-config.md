# ADR-0006: Mode enum and mode-filtered configuration

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

The same plugin binary needs to behave differently in different
contexts:

- A CI runner wants every transform enabled and every rule injected
  — full intensity, no surprises.
- A developer laptop wants a gentler default that does not
  aggressively offload when the user is debugging a specific log
  line.
- A user who is actively writing prose wants the rules layer to be
  minimal so it does not interfere with their phrasing.

A binary switch on the command line is enough for the first two. For
the third, the rules layer needs a way to filter itself based on the
active mode. The mode must also be queryable by the host adapter
(ADR-0009) so the status bar can show it, and by the orchestrator
(ADR-0005) so it can skip aggressive offloads in Lite mode.

Critically, "off" must be a real mode in the enum, not the absence
of a value. An absent mode is invisible to the status bar, invisible
to the drift test, and unqueryable from the host shim. A real mode
is a value, can be logged, can be matched on, and survives a process
restart (ADR-0015).

## Decision

### The enum

```rust
// crates/stratum-core/src/mode.rs

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Plugin is fully disabled. No transforms run, no rules emit.
    Off,
    /// Conservative: only the safest transforms run. Rules inject a
    /// single short reminder.
    Lite,
    /// Default. All registered transforms run. Full ruleset emits.
    Full,
    /// Aggressive: every transform enabled, including the optional
    /// ones. Full ruleset + worked examples.
    Ultra,
}

pub const DEFAULT_MODE: Mode = Mode::Full;

pub const ALL_MODES: &[Mode] = &[Mode::Off, Mode::Lite, Mode::Full, Mode::Ultra];

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Lite => "lite",
            Mode::Full => "full",
            Mode::Ultra => "ultra",
        }
    }

    /// Whether the orchestrator should run any transforms in this
    /// mode. `Off` means no; everything else means yes.
    pub fn runs_transforms(self) -> bool { self != Mode::Off }

    /// Maximum offload confidence threshold for this mode. Lower
    /// means more aggressive. Ultra=0.2, Full=0.5, Lite=0.8, Off=N/A.
    pub fn offload_threshold(self) -> Option<f32> {
        match self {
            Mode::Off => None,
            Mode::Lite => Some(0.8),
            Mode::Full => Some(0.5),
            Mode::Ultra => Some(0.2),
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Mode::Off),
            "lite" => Ok(Mode::Lite),
            "full" => Ok(Mode::Full),
            "ultra" => Ok(Mode::Ultra),
            other => Err(ModeParseError(other.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown mode: {0}")]
pub struct ModeParseError(pub String);
```

### Config resolver

```rust
// crates/stratum-cli/src/mode_resolver.rs (binary-side, uses env)

pub fn resolve_mode(env: &dyn EnvSource, config: &Config) -> Mode {
    // Order: env var > config file > default. Documented in the README.
    if let Some(s) = env.get("STRATUM_MODE") {
        if let Ok(m) = s.parse() {
            return m;
        }
        tracing::warn!(value = %s, "ignoring invalid STRATUM_MODE");
    }
    if let Some(m) = config.default_mode {
        return m;
    }
    DEFAULT_MODE
}
```

The env var is `STRATUM_MODE`. Resolution order: env > config file >
default. Documented in the README and in the `--help` text.

### Filter-by-mode for transforms

Each registered transform carries an optional `modes: &[Mode]`
field. The orchestrator filters by mode in addition to
`applies_to`:

```rust
// In ReformatTransform / OffloadTransform (ADR-0003):
fn modes(&self) -> &[Mode] { ALL_MODES }
```

The builder uses this to skip registration entirely when the active
mode is not in the transform's list:

```rust
impl CompressionPipelineBuilder {
    pub fn register_reformat(mut self, t: Arc<dyn ReformatTransform>, mode: Mode) -> Self {
        if t.modes().contains(&mode) { self.reformats.push(t); }
        self
    }
}
```

This means a transform author can declare "Lite only" or "Full+Ultra
only" and the orchestrator never sees it in the other modes. The
default is "all modes".

### Filter-by-mode for rules

The rules layer (ADR-0008) reads a canonical ruleset and filters by
mode. The filter is line-based, not paragraph-based, because the
canonical ruleset is structured as a markdown file with mode-tagged
sections:

```markdown
# Canonical ruleset
<!-- stratum:mode:all -->
This section applies in every mode.

## Output style
<!-- stratum:mode:full,ultra -->
Code first, then explanation.

## Intensity table
<!-- stratum:mode:ultra -->
| Level | What changes |
...
```

The builder in `stratum-hosts/src/rules.rs` parses the file once,
strips out lines whose `stratum:mode:` directive does not match the
active mode, and returns the filtered body. The HTML-comment
directive is the same pattern used by the canonical-ruleset
filter-by-mode logic; it is the minimum that works.

### Deactivation command

A user can change mode at runtime via a slash command:

```
/stratum off
/stratum lite
/stratum full
/stratum ultra
/stratum              # prints current mode
```

The parser is in `crates/stratum-hosts/src/command.rs`:

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum StratumCommand {
    Status,
    SetMode(Mode),
    SetDefault(Mode),
    Invalid(String),
}

pub fn parse_command(text: &str, current: Mode) -> StratumCommand {
    let t = text.trim();
    let body = t.strip_prefix("/stratum").unwrap_or("").trim();
    if body.is_empty() { return StratumCommand::Status; }
    match body.parse::<Mode>() {
        Ok(m) => StratumCommand::SetMode(m),
        Err(_) if body.eq_ignore_ascii_case("default") => StratumCommand::SetDefault(current),
        Err(_) => StratumCommand::Invalid(body.to_string()),
    }
}
```

Whole-message match for deactivation is enforced in ADR-0012; this
ADR only specifies the command grammar.

### Status bar

The status bar text is computed once per mode:

```rust
pub fn status_bar(mode: Mode) -> String {
    match mode {
        Mode::Off => "[STRATUM:OFF]".into(),
        Mode::Lite => "[STRATUM:LITE]".into(),
        Mode::Full => "[STRATUM]".into(),
        Mode::Ultra => "[STRATUM:ULTRA]".into(),
    }
}
```

The actual status bar script is host-specific (ADR-0009); the
*string* is owned by core.

## Consequences

Negative first:

- Four modes is a fixed number. A user who wants a fifth must
  extend the enum and accept that every adapter and the drift test
  must be updated. This is intentional (ADR-0001) — the mode enum
  is part of the plugin's identity, not a user-facing setting.
- The line-based filter for rules is fragile. A rule whose body
  happens to contain the substring `<!-- stratum:mode:` will be
  stripped if the active mode is wrong. The drift test (ADR-0008)
  asserts on every directive in the canonical file, which catches
  most accidents.
- "Off" being a real mode means every code path that handles mode
  must explicitly handle `Mode::Off`. A `match mode { ... }` that
  forgets the `Off` arm will fail to compile, which is the
  intended safety net.

Positive:

- A user can disable the plugin with `/stratum off` and re-enable
  with `/stratum full`. The status bar reflects the state. Nothing
  else in the system needs to know.
- The same binary runs at four intensity levels with no recompile,
  no separate binary, no flag day.
- The env var precedence (`STRATUM_MODE` > config file > default)
  means CI can pin a mode without modifying the config file or
  the binary.

## Implementation notes

The `Mode` enum lives in `crates/stratum-core/src/mode.rs`. The
`resolve_mode` function lives in `crates/stratum-cli/src/mode_resolver.rs`
because it depends on the env source trait, which is binary-side.

The rules filter lives in `crates/stratum-hosts/src/rules.rs`. It
parses the canonical ruleset from `docs/rules/CANONICAL.md` (which
is `include_str!`-ed into the binary at build time):

```rust
const CANONICAL: &str = include_str!("../../../docs/rules/CANONICAL.md");

pub fn build_rules(mode: Mode) -> String {
    let mut out = String::with_capacity(CANONICAL.len());
    let mut keep = true;
    for line in CANONICAL.lines() {
        if let Some(rest) = line.strip_prefix("<!-- stratum:mode:") {
            let directive = rest.strip_suffix("-->").unwrap_or(rest).trim();
            let allowed: Vec<&str> = directive.split(',').map(str::trim).collect();
            keep = match directive {
                "all" => true,
                "off" => mode == Mode::Off,
                other => other.split(',').any(|m| m == mode.as_str()),
            };
            continue;
        }
        if keep { out.push_str(line); out.push('\n'); }
    }
    out
}
```

Tests for the filter live in the same file:

1. `off_mode_strips_full_rules`
2. `lite_mode_includes_only_safe_blocks`
3. `full_mode_is_default`
4. `ultra_mode_includes_everything`
5. `unknown_mode_directive_is_treated_as_always_keep` (safety:
   better to over-include than silently drop a rule)

The CLI subcommand `/stratum default` writes the current mode to
`config.default_mode` so it persists across sessions. ADR-0015
covers the config file format.
