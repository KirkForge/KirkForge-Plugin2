# ADR-0010: Hooks integration model for AI agent hosts

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A Rust binary alone does not integrate with an AI agent host. The
host needs a manifest that declares "this is a plugin" and a way
to invoke the binary at the right lifecycle moments (session
start, before a tool call, etc.). The convention is:

1. A directory with a `plugin.json` manifest declaring the plugin's
   name, version, description, and entry points.
2. A `hooks/hooks.json` file mapping host lifecycle events to
   shell commands.
3. The shell commands invoke the binary as a short-lived process;
   the binary reads stdin (JSON), does its work, and writes stdout
   (JSON).

The plugin is metadata + glue; the binary does the work. This ADR
defines the manifest shape and the binary's hook subcommands.

## Decision

### Marketplace manifest

The repo root contains `.claude-plugin/marketplace.json`:

```json
{
  "$schema": "https://example.invalid/schemas/marketplace.schema.json",
  "name": "stratum",
  "owner": { "name": "KirkForge" },
  "plugins": [
    {
      "name": "stratum",
      "version": "0.1.0",
      "source": "./plugins/stratum-agent-hooks",
      "description": "Pipeline + behaviour layer for AI agent context."
    }
  ]
}
```

A single plugin entry, named `stratum`, sourced from the
`plugins/stratum-agent-hooks/` directory. The marketplace is the
single installable surface.

### Plugin manifest

`plugins/stratum-agent-hooks/.claude-plugin/plugin.json`:

```json
{
  "name": "stratum",
  "version": "0.1.0",
  "description": "Pipeline + behaviour layer for AI agent context.",
  "license": "MIT OR Apache-2.0",
  "keywords": ["context", "compression", "agent", "behaviour"]
}
```

No entry points declared here; the hooks file is the entry.

### Hooks manifest

`plugins/stratum-agent-hooks/hooks/hooks.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume",
        "hooks": [
          { "type": "command", "command": "stratum init hook session-start" }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          { "type": "command", "command": "stratum mode track" }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash|PowerShell",
        "hooks": [
          { "type": "command", "command": "stratum init hook ensure" }
        ]
      }
    ],
    "SubagentStart": [
      {
        "hooks": [
          { "type": "command", "command": "stratum init hook subagent" }
        ]
      }
    ]
  }
}
```

Each hook is a shell command that runs the `stratum` binary with a
subcommand. The binary reads the host's JSON payload from stdin,
processes it, and writes a JSON response to stdout (per ADR-0009).

### Binary subcommands

The binary exposes hook-specific subcommands under
`stratum init hook` and `stratum mode`:

```
stratum init hook session-start    # SessionStart handler
stratum init hook subagent         # SubagentStart handler
stratum init hook ensure           # PreToolUse handler (idempotent setup)
stratum mode track                 # UserPromptSubmit handler
stratum mode set <mode>            # explicit mode change
stratum mode status                # print current mode
```

Each subcommand is implemented in `crates/kirkstratum-cli/src/hooks/`:

```rust
// crates/kirkstratum-cli/src/hooks/session_start.rs

use anyhow::Result;
use std::io::{Read, Write};
use kirkstratum_core::mode::Mode;
use kirkstratum_hosts::rules::build_rules;
use kirkstratum_hosts::shim::{emit_to, Event, Host, Payload};

pub fn run() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let env = std::env::vars().collect::<Vec<_>>();
    let env_source = StdEnvSource::new(&env);
    let host = Host::from_env(&env_source);

    let mode = resolve_mode(&env_source, &load_config(&env_source)?);
    let rules = build_rules(mode);

    let payload = Payload::InjectRules { mode, rules };
    let response = emit_to(host, Event::SessionStart, payload);

    std::io::stdout().write_all(response.to_string().as_bytes())?;
    Ok(())
}
```

The handler is a thin wrapper: read stdin, build the payload,
emit through the shim, write stdout. No business logic.

### `init hook ensure`

`stratum init hook ensure` is the PreToolUse handler. Its job is
to make sure the local Stratum runtime is healthy before a Bash
or PowerShell tool call. It checks:

- The binary is on `PATH` (it must be, since the hook invoked it).
- The default override config is parseable, if present.
- The offload store backend is reachable, if configured.

It writes a one-line status to stdout (consumed by the host as
the tool's additional context):

```
stratum: ok (mode=full, store=sqlite:1 entries)
```

On failure:

```
stratum: error: <message>
```

The exit code is `0` on success, non-zero on failure. The host's
hook machinery decides whether non-zero blocks the tool call.

### Install / uninstall

The plugin is installed by the host's marketplace mechanism; we do
not write an installer. Uninstall is the marketplace's "remove
plugin" action, which deletes the plugin directory and the
marketplace manifest entry.

The binary itself writes state only to `$XDG_CONFIG_HOME/stratum/`
and `$XDG_DATA_HOME/stratum/` (ADR-0015). Uninstalling the plugin
does not delete that state; the README documents a
`stratum uninstall` subcommand (deferred to a future ADR) that
removes it on demand.

### Statusbar setup nudge

On first `SessionStart`, the binary checks whether the host's
status bar is configured. If not, it appends a one-time nudge to
the SessionStart output:

```
stratum: no statusline configured; run `stratum init statusline`
to enable the mode badge.
```

The nudge is opt-in via the `STRATUM_NUDGE_STATUSLINE` env var
(default `1`). Set to `0` to silence it.

## Consequences

Negative first:

- The hooks manifest is host-specific. A host that uses a
  different event name (e.g. `sessionStart` vs `SessionStart`)
  needs a parallel `hooks.json` in the plugin directory. The
  single-binary, multi-host design lives in `kirkstratum-hosts`
  (ADR-0008, ADR-0009); the plugin directory is the host-facing
  surface.
- `stratum init hook ensure` runs on every `Bash` and `PowerShell`
  tool call. The check is fast (a stat + a parse), but it is not
  free. The cost is documented in the README.
- The statusbar nudge fires on every `SessionStart` until the
  user configures the status bar. There is no "nudged already"
  flag file; the nudge is gated only on the host's settings.

Positive:

- The binary is the entire integration. The plugin manifest is
  metadata; the hooks file is glue. A new host that adopts the
  same hook protocol needs only a new `hooks.json` (or, if the
  event names differ, a new adapter in `kirkstratum-hosts`).
- The hook subcommands are short-lived processes. There is no
  daemon, no socket, no IPC. The host invokes `stratum`, Stratum
  responds, both move on.
- The `init hook ensure` pattern is the standard "verify the
  runtime is healthy before doing work" pattern. It is idempotent
  and safe to call on every tool use.

## Implementation notes

The hook subcommands live in `crates/kirkstratum-cli/src/hooks/`:

```
crates/kirkstratum-cli/src/hooks/
├── mod.rs
├── session_start.rs
├── subagent.rs
├── ensure.rs
└── mode_track.rs
```

The `CliArgs` enum (ADR-0016) is:

```rust
#[derive(Parser)]
#[command(name = "stratum", version, about)]
pub enum Cli {
    Init(InitArgs),
    Mode(ModeArgs),
    Config(ConfigArgs),
    Rules(RulesArgs),
    Version,
}

#[derive(Parser)]
pub enum InitArgs {
    Hook(HookArgs),
    Statusline,
}

#[derive(Parser)]
pub struct HookArgs {
    pub subcommand: HookSubcommand,
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum HookSubcommand {
    SessionStart,
    Subagent,
    Ensure,
}
```

The `stratum mode track` subcommand reads the user's prompt from
stdin, parses it for a `/stratum <mode>` slash command
(ADR-0006), and writes the mode to the flag file (ADR-0015).
It does not inject rules; that is the `SessionStart` handler's job.

The `stratum mode status` subcommand reads the flag file and
prints the current mode (or the default if the flag file is
absent). It is the easiest way to debug "why is Stratum in Lite
mode right now?".

The `stratum init statusline` subcommand (deferred; ADR-0010
documents the nudge but not the implementation) prints the shell
snippet the user should paste into their host's settings. The
snippet is host-specific; the drift test does not cover it.
