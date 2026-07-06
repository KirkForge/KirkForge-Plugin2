#!/usr/bin/env bash
# pre-tool-bash hook: validate the stratum configuration before any bash tool
# is invoked so configuration drift is surfaced early.
#
# Receives env vars: KF_EVENT, KF_TOOL_NAME, KF_TOOL_ARGS_JSON, KF_SESSION_ID.

set -euo pipefail

if ! command -v stratum >/dev/null 2>&1; then
  echo "[stratum hook: pre-tool-bash] stratum binary not found on PATH" >&2
  exit 0
fi

stratum config --validate
