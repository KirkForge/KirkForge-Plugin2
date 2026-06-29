# Security Policy

## Supported Versions

Only the latest commit on the `main` branch is actively supported with
security fixes. Because Stratum is currently pre-1.0 and releases are made from
`main`, users should stay on the most recent tagged release or the latest `main`.

| Version | Supported          |
| ------- | ------------------ |
| main    | :white_check_mark: |
| < main | :x:                |

## Reporting a Vulnerability

If you discover a security issue in Stratum, please report it privately by
emailing the maintainers at the address listed in the repository owner profile
or by opening a private security advisory on GitHub. Please do not disclose
security issues publicly until a fix has been released.

When reporting, include:

- A description of the vulnerability and its impact.
- Steps to reproduce or a proof-of-concept.
- The affected component (CLI, core pipeline, host adapters, etc.).
- The Stratum version or commit you tested against.

We aim to acknowledge reports within 5 business days and ship a fix or
mitigation within 30 days for critical issues.

## Security Model

Stratum is a local command-line tool that processes text on the same machine
where it runs. It does not perform network requests, store credentials, or
execute external commands as part of normal pipeline operation. The following
behaviors are intentional and relevant to security audits:

- Input is read from stdin or a user-supplied file path and is bounded by
  `--max-input-size` to limit memory pressure.
- The default config is embedded in the binary; user overrides are read from
  explicitly supplied `--config` / `--config-dir` paths or the XDG config
  directory. Unknown config keys are rejected with `EX_CONFIG` (78).
- `unsafe_code` is forbidden at the workspace level and in each crate root.
- The in-memory offload store recovers from poisoned locks rather than
  panicking, keeping the process available after an unexpected failure.

## Known limitations

- The default `InMemoryOffloadStore` is process-scoped. Offloaded payloads are
  lost when the process exits. A persistent `SqliteOffloadStore` is planned
  behind the `sqlite` feature.
- Stratum does not provide network isolation beyond the OS process model. It
  does not perform network requests as part of normal operation, but it runs
  within the same process as its host and shares the host's privileges.
- Input is bounded by `--max-input-size` to limit memory pressure, but
  transform execution is not yet time-bounded. Pathological inputs could
  block the calling thread until processing completes.

## Supply Chain

CI runs `cargo deny check` on every push and pull request to validate
dependency licenses, advisories, and source origins. See `deny.toml` for the
full policy. Dependencies are pinned via `Cargo.lock` in CI builds and
`cargo-deny` is installed with a locked version.
