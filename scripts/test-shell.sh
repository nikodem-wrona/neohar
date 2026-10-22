#!/usr/bin/env bash
# Runs the Playwright spec of tests/ui: the shell of the UI in WebKit against a headless core
# (`nh daemon --dev-ws`).
#
# It runs only locally, never in `mise run test`, because it needs a browser.
# Arguments go to `playwright test`.
#
# Needs: `node` on the PATH (the Playwright command line is a Node program), `cargo`, and the
# Playwright WebKit (`mise run setup:browsers`).
set -euo pipefail

cd "$(dirname "$0")/../tests/ui"

if ! command -v node >/dev/null 2>&1; then
  echo "test:shell needs node on the PATH (the Playwright command line is a Node program)." >&2
  exit 1
fi

exec ./node_modules/.bin/playwright test "$@"
