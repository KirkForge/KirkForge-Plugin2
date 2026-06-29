# ADR-0015: State management (XDG, flag file, no install DB)

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

Stratum needs to remember two things across invocations:

1. The user's default mode (ADR-0006).
2. The current mode of an active session (so the status bar
   reflects it).

It must not need:

- An install database.
- A daemon process.
- A socket.
- Network access.

The state lives on disk in well-known locations, with the
minimum possible surface. The convention is XDG Base Directory
on Linux/macOS and the equivalent on Windows. The single config
file is human-editable TOML; the single flag file is a plain
string written atomically.

A misbehaving uninstall must clean up state. The state must be
discoverable (`stratum config show`, `stratum mode status`) and
removable (`stratum uninstall`, deferred).

## Decision

### Directory layout

On Linux and macOS:

```
$XDG_CONFIG_HOME/stratum/             # default: ~/.config/stratum/
├── config.toml                       # main config (TOML, ADR-0007)
└── pipeline.toml                     # pipeline overrides (ADR-0007)

$XDG_DATA_HOME/stratum/               # default: ~/.local/share/stratum/
├── offload.sqlite                    # SqliteOffloadStore default location
└── offload/                          # FileOffloadStore default location
    ├── 00/
    ├── 01/
    └── ...

$XDG_RUNTIME_DIR/stratum/             # default: /run/user/<uid>/stratum/
└── mode                              # flag file: single line, the active mode
```

On Windows, the equivalent paths use `%APPDATA%\stratum\`,
`%LOCALAPPDATA%\stratum\`, and the runtime dir resolves to
`%TEMP%\stratum\` (the runtime dir is best-effort; on Windows
it is not guaranteed).

The `directories` crate (a workspace dependency) resolves the
XDG paths cross-platform.

### Config file

`config.toml` is the main config. It is parsed by the binary
at startup:

```toml
# ~/.config/stratum/config.toml
default_mode = "full"            # Mode::Full (ADR-0006)
store = "sqlite"                  # OffloadBackendConfig (ADR-0004)
log_level = "info"                # tracing filter

[overrides]
# Override file path; default is ~/.config/stratum/pipeline.toml
pipeline_config = "~/.config/stratum/pipeline.toml"
```

The struct:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub default_mode: Option<Mode>,
    pub store: Option<StoreKind>,
    pub log_level: Option<String>,
    pub overrides: Option<Overrides>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    pub pipeline_config: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreKind {
    Memory,
    Sqlite,
    File,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_mode: Some(DEFAULT_MODE),
            store: Some(StoreKind::Sqlite),
            log_level: Some("info".to_string()),
            overrides: None,
        }
    }
}
```

### Flag file

The flag file holds the active mode for the current session. It
is a single line, no JSON:

```
$ cat ~/.local/share/stratum/mode
full
```

The flag file is written atomically via
`tempfile::NamedTempFile` + `persist`:

```rust
pub fn write_mode(mode: Mode) -> std::io::Result<()> {
    let path = runtime_dir()?.join("mode");
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    std::fs::write(tmp.path(), mode.as_str())?;
    tmp.persist(&path)?;
    Ok(())
}

pub fn read_mode() -> Option<Mode> {
    let path = runtime_dir().ok()?.join("mode");
    let text = std::fs::read_to_string(&path).ok()?;
    text.trim().parse().ok()
}
```

Atomic write prevents a partial read on a crash mid-write. The
flag file is the only place where atomic write matters; the
config file is read once at startup.

### Path resolution

```rust
use directories::ProjectDirs;

pub fn config_dir() -> std::io::Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("dev", "kirkforge", "stratum")
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound, "no home directory"));
    Ok(dirs.config_dir().to_path_buf())
}

pub fn data_dir() -> std::io::Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("dev", "kirkforge", "stratum")
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound, "no home directory"));
    Ok(dirs.data_dir().to_path_buf())
}

pub fn runtime_dir() -> std::io::Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("dev", "kirkforge", "stratum")
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound, "no home directory"));
    Ok(dirs.runtime_dir().map(|p| p.to_path_buf()).unwrap_or_else(|| {
        // Windows fallback: use data_dir() if runtime_dir() is None.
        dirs.data_dir().to_path_buf()
    }))
}
```

The qualifier `"dev", "kirkforge", "stratum"` is the
`directories` convention. On Linux it produces
`~/.config/stratum/`, `~/.local/share/stratum/`, and
`/run/user/<uid>/stratum/`. On macOS it produces
`~/Library/Application Support/dev.kirkforge.stratum/`. On
Windows it produces `%APPDATA%\dev\kirkforge\stratum\`.

### Host-specific overrides

Some hosts set their own data dir env vars
(`$CLAUDE_CONFIG_DIR`, `$CODEX_HOME`, `$COPILOT_PLUGIN_DATA`).
Stratum does *not* write its state inside those directories; it
writes inside its own XDG dirs. This is the loud-failure
philosophy (ADR-0004): the state has a clear owner (Stratum)
and a clear location (the XDG dirs), and nothing else touches
it.

The host env vars are read only for *detection* (ADR-0009), not
for state paths.

### Env var override precedence

For the config directory:

1. `STRATUM_CONFIG_DIR` (explicit override).
2. XDG-resolved default (`directories` crate).

For the data directory:

1. `STRATUM_DATA_DIR` (explicit override).
2. XDG-resolved default.

For the runtime directory:

1. `STRATUM_RUNTIME_DIR` (explicit override).
2. XDG-resolved default.

The env vars exist for tests and for users who want to sandbox
the plugin (e.g. `STRATUM_DATA_DIR=/tmp/stratum-test`).

### `stratum mode track` writes the flag

The `mode track` subcommand (ADR-0010) parses the user's prompt
for a `/stratum <mode>` slash command and, if found, writes the
new mode to the flag file. It does not write the config file —
the config file holds the *default*, the flag file holds the
*current* (which may differ from the default for the duration
of a session).

### Resolution at startup

```rust
pub fn resolve_mode(env: &dyn EnvSource, config: &Config) -> Mode {
    // 1. STRATUM_MODE env var (per-invocation override).
    if let Some(s) = env.get("STRATUM_MODE") {
        if let Ok(m) = s.parse() { return m; }
    }
    // 2. Flag file (current session).
    if let Some(m) = read_mode() { return m; }
    // 3. Config file default.
    if let Some(m) = config.default_mode { return m; }
    // 4. Hard-coded default.
    DEFAULT_MODE
}
```

`STRATUM_MODE` is the per-invocation override (a CI runner can
set it once and not worry about state). The flag file is the
session override (the user ran `/stratum lite` an hour ago and
the runtime is still in Lite). The config file is the persistent
default. The hard-coded default is the fallback when nothing
else applies.

### `stratum uninstall` (deferred)

The uninstall subcommand is deferred. When implemented, it
removes:

- `$XDG_CONFIG_HOME/stratum/`
- `$XDG_DATA_HOME/stratum/`
- `$XDG_RUNTIME_DIR/stratum/mode`

It does not remove anything outside those paths. The plugin
manifest (ADR-0010) is removed by the host's marketplace
mechanism, not by Stratum.

## Consequences

Negative first:

- Three directories (config, data, runtime) is one more than
  the minimum. A user who wants a single directory must set all
  three env vars. The README documents the env vars.
- The flag file is per-session, not per-host. A user who runs
  Stratum from two hosts simultaneously will see one of them
  overwrite the other's flag file. The detection logic
  (ADR-0009) does not currently scope the flag file by host;
  this is a deferred enhancement.
- Atomic write via `tempfile::NamedTempFile` + `persist` is
  POSIX-only. On Windows the behaviour is "best effort". This
  is acceptable because the flag file holds a single string
  and a torn write would produce a parse error, which the
  resolver treats as "no flag file".

Positive:

- The state surface is small and discoverable. Three
  directories, three files, one flag.
- The env var overrides (`STRATUM_CONFIG_DIR`, etc.) make
  testing trivial: a test sets the env vars, runs the binary,
  asserts on the resulting state in a temp dir.
- Loud failure on missing home directory: `ProjectDirs::from`
  returns `None` on a system with no `$HOME`, and the binary
  exits with code `78` and a clear message. There is no silent
  fallback to `/tmp/stratum`.

## Implementation notes

The path resolution lives at `crates/kirkstratum-core/src/paths.rs`.
The config struct lives at `crates/kirkstratum-cli/src/config.rs`.
The flag file helpers live at `crates/kirkstratum-cli/src/flag.rs`.

The `directories` crate is a workspace dependency:

```toml
# Cargo.toml workspace
directories = "5"
```

The `tempfile` crate is a dependency of the CLI only:

```toml
# crates/kirkstratum-cli/Cargo.toml
tempfile = "3"
```

Tests:

1. `config_dir_resolves_to_xdg_path` — on Linux, assert
   `config_dir()` returns `~/.config/stratum` (or `$XDG_CONFIG_HOME/stratum` if set).
2. `flag_file_round_trip` — write a mode, read it back, assert
   equality.
3. `atomic_write_does_not_leave_partial_file` — write a mode
   while a reader is reading; assert the reader sees either the
   old value or the new value, never a partial.
4. `resolve_mode_precedence` — set env, flag, and config to
   different modes; assert env wins.
5. `missing_home_returns_io_error` — unset `$HOME`, assert
   `config_dir()` returns `Err`.
6. `uninstall_removes_all_three_dirs` (deferred).

Test 3 is a stress test with many concurrent writes and reads;
it is marked `#[ignore]` by default and run only on CI.
