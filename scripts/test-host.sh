#!/usr/bin/env bash
# Run the check of the sandbox of the webview against a debug build of the app.
# The build has the frontend inside it (`cargo tauri build --debug`), so the page has the CSP of a
# product build. The app opens a window, runs a script in it, prints what the page could do, and
# ends with the code 0 when every rule holds. It uses temporary directories.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
cargo build -p nh
(cd apps/desktop && cargo tauri build --no-bundle --debug --features selftest)

dir="$(mktemp -d)"
run="/tmp/nhs-$(id -u)-$$"
mkdir -m 700 -p "$run"
trap 'rm -rf "$dir" "$run"' EXIT

status=0
NEOHAR_CONFIG_DIR="$dir/config" NEOHAR_DATA_DIR="$dir/data" NEOHAR_RUNTIME_DIR="$run" \
  NEOHAR_NH="$root/target/debug/nh" NEOHAR_SELFTEST=1 \
  timeout 90 "$root/target/debug/neohar-desktop" | tee "$dir/out" || status=$?
# A second launch hands over to a neohar app that runs and ends with 0 (single instance), so the
# check passes only with the report of the page.
if [ "$status" -eq 0 ] && ! grep -q '"rpc"' "$dir/out"; then
  echo "test:host: the self-test did not run. Another neohar app runs: quit it first." >&2
  status=1
fi
exit "$status"
