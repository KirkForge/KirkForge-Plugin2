# ADR-0016: CLI design — clap-derive, env override precedence

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

The binary is the user-facing surface. Every choice in the CLI
shows up in `--help`, in the README, in shell completion, and in
the muscle memory of every operator. The CLI must be:

- Discoverable: `--help` on every subcommand lists every flag.
- Consistent: every flag has the same name across subcommands.
- Scriptable: every subcommand accepts `--json` for machine-readable
  output.
- Safe: destructive operations require `--yes` or are deferred.
- Fast: the binary starts in <50 ms. No slow first-time setup.

`clap` with the `derive` feature is the right tool. It generates
`--help`, shell completion, and parses args into a typed struct.
The alternative (hand-rolled arg parsing) is faster to write once
and slower to maintain forever.

## Decision

### Top-level structure

The CLI is one binary with subcommands. The top-level enum:

```rust
// crates/kirkstratum-cli/src/args.rs

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "stratum", version, about = "Pipeline + behaviour layer for AI agent context.")]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Decrease verbosity.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub quiet: u8,

    /// Emit machine-readable JSON to stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Path to config directory (default: $XDG_CONFIG_HOME/stratum).
    #[arg(long, env = "STRATUM_CONFIG_DIR", global = true)]
    pub config_dir: Option<std::path::PathBuf>,

    /// Path to data directory (default: $XDG_DATA_HOME/stratum).
    #[arg(long, env = "STRATUM_DATA_DIR", global = true)]
    pub data_dir: Option<std::path::PathBuf>,

    /// Path to runtime directory (default: $XDG_RUNTIME_DIR/stratum).
    #[arg(long, env = "STRATUM_RUNTIME_DIR", global = true)]
    pub runtime_dir: Option<std::path::PathBuf>,

    /// Path to config file (overrides XDG default; ADR-0007).
    #[arg(long, env = "STRATUM_CONFIG", global = true)]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialise runtime (hooks, statusline).
    Init(InitArgs),
    /// Read or set the active mode.
    Mode(ModeArgs),
    /// Inspect or print the effective config.
    Config(ConfigArgs),
    /// Inspect or print the ruleset for a mode.
    Rules(RulesArgs),
    /// Harvest the deferred-work ledger.
    Debt(DebtArgs),
    /// Apply the pipeline to a file or stdin (debugging).
    Apply(ApplyArgs),
    /// Print version and exit.
    Version,
}
```

### Subcommand shapes

```rust
#[derive(Parser, Debug)]
pub enum InitArgs {
    /// Run a host hook handler (session-start, subagent, ensure).
    Hook(HookArgs),
    /// Print the statusline setup snippet for the detected host.
    Statusline,
    /// Initialise the default config in $XDG_CONFIG_HOME/stratum/.
    Config,
}

#[derive(Parser, Debug)]
pub struct HookArgs {
    pub subcommand: HookSubcommand,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum HookSubcommand {
    SessionStart,
    Subagent,
    Ensure,
}

#[derive(Parser, Debug)]
pub enum ModeArgs {
    /// Print the current mode to stdout.
    Status,
    /// Set the mode for this invocation (env var equivalent).
    Set { mode: Mode },
    /// Persist the current mode as the default in config.toml.
    SetDefault { mode: Mode },
    /// Track a user prompt for slash commands (used by the UserPromptSubmit hook).
    Track,
}

#[derive(Parser, Debug)]
pub enum ConfigArgs {
    /// Print the effective config (defaults + overrides) to stdout.
    Show,
    /// Print the source of each field.
    ShowSources,
    /// Validate the config and exit non-zero on errors.
    Validate,
}

#[derive(Parser, Debug)]
pub enum RulesArgs {
    /// Print the ruleset for a mode to stdout.
    Show { mode: Mode },
    /// Print the canonical ruleset (unfiltered) to stdout.
    Canonical,
}

#[derive(Parser, Debug)]
pub struct DebtArgs {
    /// Write to file instead of stdout.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,
}

#[derive(Parser, Debug)]
pub struct ApplyArgs {
    /// File to read from; default is stdin.
    pub file: Option<std::path::PathBuf>,

    /// Force content type detection (skip the layered detect; ADR-0014).
    #[arg(long, value_enum)]
    pub content_type: Option<ContentTypeArg>,

    /// Override the mode for this invocation.
    #[arg(long)]
    pub mode: Option<Mode>,

    /// Print pipeline audit trail to stderr.
    #[arg(long)]
    pub verbose: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ContentTypeArg {
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

### Precedence chain (env > CLI > file > default)

For every value that has multiple sources, the resolution order is:

1. `STRATUM_*` env var (highest).
2. Explicit CLI flag (e.g. `--config path.toml`).
3. XDG default file (`$XDG_CONFIG_HOME/stratum/config.toml`).
4. Hard-coded default.

The precedence is documented in the README and asserted by the
test suite. A refactor that reorders these layers fails CI.

```rust
// crates/kirkstratum-cli/src/precedence.rs

pub fn resolve_config_path(
    cli: &Cli, env: &dyn EnvSource, xdg: &std::path::Path,
) -> std::path::PathBuf {
    if let Some(p) = &cli.config { return p.clone(); }             // CLI flag
    if let Some(p) = env.get("STRATUM_CONFIG") {                    // env
        return std::path::PathBuf::from(p);
    }
    xdg.join("config.toml")                                         // XDG default
}
```

### `--json` for every subcommand

Every subcommand that produces structured output accepts `--json`
at the top level. The subcommand decides whether to emit JSON or
human-readable text based on `cli.json`:

```rust
fn emit<T: serde::Serialize>(cli: &Cli, human: &str, machine: &T) {
    if cli.json {
        println!("{}", serde_json::to_string_pretty(machine).unwrap());
    } else {
        print!("{}", human);
    }
}
```

`--json` is *additive*: a subcommand that does not have a
machine-readable form ignores it. A subcommand that always emits
JSON (e.g. `mode status` when `--json` is set) emits JSON; when
omitted, it emits the human form.

### Exit codes

The binary's exit codes follow the BSD sysexits convention where
applicable:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Generic failure |
| 64 | Usage error (clap parses this for us) |
| 78 | `EX_CONFIG` — config parse or backend init failure |
| 130 | `SIGINT` (default) |

```rust
// crates/kirkstratum-cli/src/exit.rs

pub fn exit_config_err(msg: &str) -> ! {
    eprintln!("stratum: {}", msg);
    std::process::exit(78);
}

pub fn exit_usage_err(msg: &str) -> ! {
    eprintln!("stratum: {}", msg);
    std::process::exit(64);
}
```

### Shell completion

`clap` generates shell completion at build time via the
`clap_complete` crate. The completion scripts are written to
`target/stratum-completion.{bash,zsh,fish}` and installed via
the `--install-completion` subcommand (deferred; the build step
is enough for MVP).

### Help output conventions

Every subcommand's `--help` lists:

1. A one-line description.
2. Usage line with all positional and required args.
3. Options section with every flag, default, and env var binding.
4. Examples section with 2–4 common invocations.
5. See also: link to the relevant ADR.

The example:

```
$ stratum mode --help
Read or set the active mode.

Usage: stratum mode <SUBCOMMAND>

Options:
  -h, --help     Print help

Subcommands:
  status         Print the current mode to stdout
  set            Set the mode for this invocation
  set-default    Persist the current mode as the default in config.toml
  track          Track a user prompt for slash commands (used by UserPromptSubmit hook)

Examples:
  # Print the current mode
  $ stratum mode status

  # Set the mode to Lite for this invocation
  $ stratum mode set lite

  # Persist Lite as the new default
  $ stratum mode set-default lite

See also: ADR-0006 (mode enum), ADR-0015 (state management)
```

The `See also` line is the load-bearing bit. A user reading the
help text knows exactly which ADR documents the design.

## Consequences

Negative first:

- The CLI surface is large. Every subcommand is one more
  surface to test, document, and maintain. The trade is that
  every user-visible action is a discoverable subcommand rather
  than a hidden flag.
- `clap` is a heavy dependency (~150 KB compiled). The binary
  is larger than it would be with hand-rolled parsing. The
  trade is `--help` quality and shell completion for free.
- The precedence chain is enforced by code, not by clap. A
  refactor that bypasses `resolve_config_path` could silently
  change behaviour. The test suite catches this; the review
  must catch it earlier.

Positive:

- `--help` is comprehensive, consistent, and includes ADR
  cross-references.
- `--json` on every subcommand makes the binary scriptable.
- Exit codes follow sysexits; CI and ops tools can branch on
  them.
- Env > CLI > file > default precedence is documented and
  asserted by tests.

## Implementation notes

The CLI lives at `crates/kirkstratum-cli/src/`. Subcommand
implementations live in submodules:

```
crates/kirkstratum-cli/src/
├── main.rs          # tokio::main / clap parse / dispatch
├── args.rs          # Cli, Command, subcommand arg structs
├── precedence.rs    # resolve_config_path, resolve_mode, etc.
├── exit.rs          # exit_config_err, exit_usage_err
├── config_loader.rs # load_config (ADR-0007)
├── mode_resolver.rs # resolve_mode (ADR-0006)
├── hooks/
│   ├── mod.rs
│   ├── session_start.rs
│   ├── subagent.rs
│   └── ensure.rs
├── config/
│   ├── mod.rs
│   ├── show.rs
│   └── validate.rs
├── rules.rs         # rules show, canonical
├── debt.rs          # debt harvest
├── apply.rs         # apply pipeline to stdin/file
└── state.rs         # flag file read/write (ADR-0015)
```

The `clap` features enabled in `Cargo.toml`:

```toml
[dependencies.clap]
version = "4"
features = ["derive", "env", "string", "unicode"]
```

The `env` feature is what binds `--config` to `STRATUM_CONFIG`
automatically. The `derive` feature is what generates `Cli` from
the struct. The `string` and `unicode` features are defaults.

Tests:

1. `resolve_config_path_cli_wins_over_xdg` — assert CLI flag
   wins.
2. `resolve_config_path_env_wins_over_cli` — assert env wins.
3. `mode_status_emits_human_by_default`
4. `mode_status_emits_json_when_json_flag_set`
5. `unknown_subcommand_exits_64` — clap handles this; assert
   the exit code.
6. `config_validate_exits_78_on_unknown_field` — assert the
   `EX_CONFIG` exit code.
7. `apply_stdin_runs_pipeline` — pipe JSON into `stratum apply`,
   assert the transformed output is on stdout.

Test 2 is the regression for the precedence chain. A
contributor who swaps the two `if let` arms silently breaks
the precedence; the test fails immediately.