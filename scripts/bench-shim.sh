#!/usr/bin/env bash
# The budget runs of the hook shim: the start time of `nh hook claude Stop`, called
# through a symlink as hooks call it. Three runs, each with 500 samples after 20 warm-up runs:
#   1. a core is running      p50 <= 5 ms and p99 <= 15 ms
#   2. the socket is a leftover that no core listens on      p50 <= 5 ms
#   3. the socket does not exist      p50 <= 5 ms
# `nh` is the release build of this checkout, or the binary named in NH_BIN (for example the one
# inside neohar.app). The core is the real nh-core without a window (crates/nh/examples/api_core).
# Everything lives in a temp directory. Local only.
# Usage: scripts/bench-shim.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release --locked -p nh
cargo build --release --locked -p nh --example api_core

# Short: the temp directory holds a Unix socket (macOS allows 103 bytes).
tmp="$(mktemp -d /tmp/nh-s4.XXXXXX)"
core_pid=""
cleanup() {
  [ -n "$core_pid" ] && kill "$core_pid" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT

nh_bin="${NH_BIN:-$root/target/release/nh}"
mkdir -p "$tmp/bin" "$tmp/run"
chmod 700 "$tmp/run"
ln -s "$nh_bin" "$tmp/bin/nh"
out="$root/target/bench-shim"
mkdir -p "$out"
payload="$root/scripts/fixtures/hook-stop.json"

target/release/examples/api_core "$tmp/run" s4 >"$tmp/core.out" &
core_pid=$!
for _ in $(seq 1 100); do
  grep -q '^READY ' "$tmp/core.out" 2>/dev/null && break
  sleep 0.1
done
socket="$(sed -n 's/^READY //p' "$tmp/core.out")"
[ -S "$socket" ] || { echo "bench-shim: the core did not start" >&2; exit 1; }

echo "== 1. a core is running ($nh_bin through $tmp/bin/nh)"
bun scripts/shim-bench.ts --nh "$tmp/bin/nh" --socket "$socket" --payload "$payload" \
  --p50 5 --p99 15 --out "$out/running.json"

# The measurement is valid only if the core received what the shim sent.
grep -q '^SIGNAL claude Stop$' "$tmp/core.out" \
  || { echo "bench-shim: the core received no signal; run 1 measured nothing" >&2; exit 1; }

kill -9 "$core_pid"
wait "$core_pid" 2>/dev/null || true
core_pid=""
[ -S "$socket" ] || { echo "bench-shim: the killed core left no socket file" >&2; exit 1; }

echo "== 2. a leftover socket file, no core"
bun scripts/shim-bench.ts --nh "$tmp/bin/nh" --socket "$socket" --payload "$payload" \
  --p50 5 --out "$out/leftover.json"

echo "== 3. no socket file"
bun scripts/shim-bench.ts --nh "$tmp/bin/nh" --socket "$tmp/missing.sock" --payload "$payload" \
  --p50 5 --out "$out/missing.json"

echo "bench-shim: reports in $out"
