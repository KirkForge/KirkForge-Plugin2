# ADR-0007: TOML config embedded via `include_str!`

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

Every binary needs default configuration. The question is where that
default lives:

1. Hard-coded in Rust source. Simple, but the user cannot inspect
   or override without recompiling.
2. In a separate `.toml` file shipped alongside the binary. Easy
   to inspect, but the binary does not work out-of-the-box if the
   file is missing or in the wrong place.
3. Embedded in the binary via `include_str!` and overrideable at
   runtime via a TOML file or CLI flag. Works out-of-the-box, easy
   to inspect with `stratum config show`, and a power user can drop
   a `~/.config/stratum/pipeline.toml` to override any value.

Option 3 is the pattern. The defaults must be visible to the user
without reading the source. A `stratum config show` subcommand that
prints the merged effective config (defaults + overrides) is the
load-bearing UX feature.

## Decision

### Embedded default

The defaults live at `crates/stratum-core/config/pipeline.toml`:

```toml
# Default Stratum pipeline config. Embedded via include_str!.
# Override at runtime with `--config path/to.toml`.

reformat_target_ratio = 0.05
bloat_threshold = 0.5
offload_fallback_ratio = 0.85

[per_domain.bloat.log]
bloat_threshold = 0.3
reformat_target_ratio = 0.1

[per_domain.bloat.diff]
bloat_threshold = 0.6

[per_domain.bloat.search]
bloat_threshold = 0.4

[per_domain.bloat.json_array]
bloat_threshold = 0.5

[per_domain.reformat.log_template]
bloat_threshold = 0.2
```

The file is embedded into the binary via:

```rust
// crates/stratum-core/src/pipeline/config.rs

pub const DEFAULT_TOML: &str = include_str!("../../config/pipeline.toml");

impl Default for PipelineConfig {
    fn default() -> Self {
        Self::from_str(DEFAULT_TOML)
            .expect("embedded config/pipeline.toml must parse")
    }
}
```

The `expect` is safe because the embedded string is compile-time
constant; the unit test `embedded_config_parses` exercises it on
every build.

### Override file

A user can drop a TOML file at:

- `--config <path>` (CLI flag, highest precedence)
- `~/.config/stratum/pipeline.toml` (XDG-resolved, mid precedence)
- `STRATUM_CONFIG` env var (highest precedence among env-driven)

The merge is a shallow per-field overwrite with deep merge for
`[per_domain]` tables. The merged config is what the orchestrator
sees; the user can see the merge result via `stratum config show`:

```
$ stratum config show
reformat_target_ratio = 0.05           # default
bloat_threshold      = 0.4             # override: ~/.config/stratum/pipeline.toml
offload_fallback_ratio = 0.85          # default
[per_domain]
  bloat.log.bloat_threshold  = 0.25    # override
  bloat.diff.bloat_threshold = 0.6     # default
  ...
```

### The `PipelineConfig` struct

```rust
// crates/stratum-core/src/pipeline/config.rs

use serde::Deserialize;
use std::collections::HashMap;

use crate::content::ContentType;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub reformat_target_ratio: f32,
    pub bloat_threshold: f32,
    pub offload_fallback_ratio: f32,
    #[serde(default)]
    pub per_domain: HashMap<ContentType, DomainOverrides>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainOverrides {
    pub bloat_threshold: Option<f32>,
    pub reformat_target_ratio: Option<f32>,
}

impl PipelineConfig {
    pub fn from_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn from_file(p: &std::path::Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(p).map_err(ConfigError::Io)?;
        Self::from_str(&text).map_err(ConfigError::Parse)
    }

    /// Merge `other` into `self`, with `other`'s fields winning on
    /// conflict. Used for the override-file merge.
    pub fn merge(&mut self, other: &PipelineConfig) {
        if other.reformat_target_ratio != 0.0 {
            self.reformat_target_ratio = other.reformat_target_ratio;
        }
        if other.bloat_threshold != 0.0 {
            self.bloat_threshold = other.bloat_threshold;
        }
        if other.offload_fallback_ratio != 0.0 {
            self.offload_fallback_ratio = other.offload_fallback_ratio;
        }
        for (ct, override_d) in &other.per_domain {
            let entry = self.per_domain.entry(*ct).or_default();
            if let Some(v) = override_d.bloat_threshold { entry.bloat_threshold = Some(v); }
            if let Some(v) = override_d.reformat_target_ratio {
                entry.reformat_target_ratio = Some(v);
            }
        }
    }
}
```

`#[serde(deny_unknown_fields)]` is the structural defence: a typo
in the override file (`bloat_threashold = 0.4`) is a startup error,
not a silent default.

### Precedence chain

The CLI binary resolves the effective config in this order, top wins:

1. `STRATUM_CONFIG` env var (path to override TOML)
2. `--config <path>` CLI flag
3. `$XDG_CONFIG_HOME/stratum/pipeline.toml` (default override path)
4. Embedded `pipeline.toml`

The resolution function:

```rust
// crates/stratum-cli/src/config_loader.rs

pub fn load_config(cli: &CliArgs, env: &dyn EnvSource) -> Result<PipelineConfig, ConfigError> {
    let mut cfg = PipelineConfig::default();

    // Layer 3: XDG default override, if present.
    if let Some(p) = default_override_path(env) {
        if p.exists() {
            let override_cfg = PipelineConfig::from_file(&p)?;
            cfg.merge(&override_cfg);
        }
    }

    // Layer 2: --config flag, if present.
    if let Some(p) = &cli.config {
        let override_cfg = PipelineConfig::from_file(p)?;
        cfg.merge(&override_cfg);
    }

    // Layer 1: STRATUM_CONFIG env var, if set (wins over --config).
    if let Some(p) = env.get("STRATUM_CONFIG") {
        let override_cfg = PipelineConfig::from_file(Path::new(&p))?;
        cfg.merge(&override_cfg);
    }

    Ok(cfg)
}
```

### `stratum config show`

The subcommand prints the merged effective config to stdout in the
same TOML format the binary would parse. Implementation:

```rust
// crates/stratum-cli/src/main.rs (in Commands::Config)

fn run_config_show(cfg: &PipelineConfig) -> anyhow::Result<()> {
    print!("{}", toml::to_string_pretty(cfg)?);
    Ok(())
}
```

The same subcommand accepts `--source` which prints the path each
field came from (default / XDG / CLI / env). This is the
diagnostic for "why is my config different from what I expected".

## Consequences

Negative first:

- Three layers of override (XDG, CLI, env) means a misconfigured
  CI can hide a problem behind two layers of overrides. The
  `stratum config show --source` subcommand is the answer; it is
  documented in the README troubleshooting section.
- The merge is per-field, not per-table. If a user sets
  `[per_domain.bloat.log]` in their override, they must repeat the
  full path; they cannot set `[per_domain]` and inherit
  sub-tables. This is a deliberate trade for `deny_unknown_fields`
  (any key in the override file is explicit, no implicit merging).
- The embedded TOML is compiled into the binary. A user who wants
  to inspect the *defaults* without running the binary must read
  the source file `crates/stratum-core/config/pipeline.toml`.
  That file is the single source of truth — editing the source
  edits the embedded string.

Positive:

- The binary works out-of-the-box with no configuration file. A
  user can drop a single `pipeline.toml` in `~/.config/stratum/`
  and have it picked up automatically.
- `deny_unknown_fields` catches typos at parse time. A user who
  fat-fingers `bloat_threashold` sees the error before the
  pipeline ever runs.
- `stratum config show` is the killer diagnostic. It is one
  subcommand away and produces copy-pasteable output.

## Implementation notes

The TOML file lives at `crates/stratum-core/config/pipeline.toml`.
The struct lives at `crates/stratum-core/src/pipeline/config.rs`.
The CLI loader lives at `crates/stratum-cli/src/config_loader.rs`.

The `ConfigError` enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error reading {path}: {source}")]
    Io { path: std::path::PathBuf, source: std::io::Error },
    #[error("parse error in {path}: {source}")]
    Parse { path: std::path::PathBuf, source: toml::de::Error },
    #[error("invalid value for {field}: {message}")]
    Invalid { field: String, message: String },
}
```

`ConfigError` is the only error type the CLI surfaces to the user
verbatim. The binary exits with code `78` (`EX_CONFIG`) on
`ConfigError`.

Tests:

1. `embedded_config_parses` — `PipelineConfig::from_str(DEFAULT_TOML).is_ok()`.
2. `merge_overrides_default` — start with defaults, merge an
   override, assert each field matches the override.
3. `merge_per_domain_deep` — override only `bloat.log`, assert
   `bloat.diff` is unchanged.
4. `unknown_field_in_override_is_error` — parse a TOML with
   `bloat_threashold = 0.4`, assert `ConfigError::Parse`.
5. `cli_config_overrides_xdg_config` — assert the precedence
   chain in `load_config`.
6. `env_config_overrides_cli_config` — assert
   `STRATUM_CONFIG` wins over `--config`.

Test 6 is the easiest to break; it is the single line
`if let Some(p) = env.get("STRATUM_CONFIG")` after the CLI flag
handling. A refactor that reorders the two silently breaks the
precedence. The test exists to make the breakage loud.
