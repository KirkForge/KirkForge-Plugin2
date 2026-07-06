#!/usr/bin/env bash
# session-start hook: emit the stratum ruleset for the active mode so the model
# knows the compression contract at the start of the session.
#
# Receives env vars: KF_EVENT, KF_SESSION_ID.

set -euo pipefail

if ! command -v stratum >/dev/null 2>&1; then
  echo "[stratum hook: session-start] stratum binary not found on PATH" >&2
  exit 0
fi

stratum rules
