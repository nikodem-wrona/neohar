#!/usr/bin/env bash
# Runs the Playwright spec of tests/transport: the WebSocketTransport in WebKit against a headless
# core (`nh daemon --dev-ws`).
#
# It runs only locally, never in `mise run test`, because it needs a browser.
# Arguments go to `playwright test`.
#
# Needs: `node` on the PATH (the Playwright command line is a Node program), `cargo`, and the
# Playwright WebKit (`mise run setup:browsers`).
set -euo pipefail

cd "$(dirname "$0")/../tests/transport"

if ! command -v node >/dev/null 2>&1; then
  echo "test:transport needs node on the PATH (the Playwright command line is a Node program)." >&2
  exit 1
fi

exec ./node_modules/.bin/playwright test "$@"
