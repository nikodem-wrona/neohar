#!/usr/bin/env bash
# Runs the Playwright specs of tests/render: the two renderers agree cell by cell, a lost WebGL
# context comes back, a frame does not allocate, and the render budgets hold.
#
# They need a real GPU, so they never run in `mise run test`. Arguments go to `playwright test`,
# for example `--project=chromium` or `perf.pw.ts`.
#
# Needs: `node` on the PATH (the Playwright command line is a Node program), `cargo` (it builds
# `nh-bench`) and the browsers of Playwright (`mise run setup:browsers`). To use a Chromium that
# you have already, set NEOHAR_CHROMIUM to its executable.
set -euo pipefail

cd "$(dirname "$0")/../tests/render"

if ! command -v node >/dev/null 2>&1; then
  echo "test:gpu needs node on the PATH (the Playwright command line is a Node program)." >&2
  exit 1
fi

export NEOHAR_GPU=1
exec ./node_modules/.bin/playwright test "$@"
