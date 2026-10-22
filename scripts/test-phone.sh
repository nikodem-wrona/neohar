#!/usr/bin/env bash
# Runs the Playwright spec of tests/phone: the phone app in WebKit as an iPhone 17 Pro Max (and as
# an iPhone 15 for one test of the narrow width) against a headless core with the listener of the
# phone (`nh daemon --dev-ws --phone-port 0`).
#
# It runs only locally, never in `mise run test`, because it needs a browser.
# Arguments go to `playwright test`.
#
# Needs: `node` on the PATH (the Playwright command line is a Node program), `cargo`, `bun`, and
# the Playwright WebKit (`mise run setup:browsers`).
set -euo pipefail

cd "$(dirname "$0")/../tests/phone"

if ! command -v node >/dev/null 2>&1; then
  echo "test:phone needs node on the PATH (the Playwright command line is a Node program)." >&2
  exit 1
fi

exec ./node_modules/.bin/playwright test "$@"
