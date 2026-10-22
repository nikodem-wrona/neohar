#!/usr/bin/env bash
# The remote tests: every crates/nh/tests/remote_*.rs, over real ssh to a second machine.
# They need that machine (NEOHAR_REMOTE_TARGET) and never run in `mise run test`. Without NEOHAR_REMOTE=1 a
# test skips itself; this task requires the variables, so a skip is not taken for a pass.
#
# Usage: NEOHAR_REMOTE_TARGET=<ssh target> mise run test:remote [-- <nextest args>]
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -z "${NEOHAR_REMOTE_TARGET:-}" ]; then
  echo "test:remote: set NEOHAR_REMOTE_TARGET to the ssh target of the remote machine," >&2
  echo "  for example mac.example.ts.net (the full name: the short name" >&2
  echo "  fails the host key check)" >&2
  exit 2
fi
export NEOHAR_REMOTE=1

tests=()
for file in crates/nh/tests/remote_*.rs; do
  name="$(basename "$file" .rs)"
  tests+=(--test "$name")
done

exec bash scripts/with-test-env.sh cargo nextest run --locked -p nh "${tests[@]}" --no-capture --no-fail-fast "$@"
