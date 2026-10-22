#!/usr/bin/env bash
# Run a command with NEOHAR_CONFIG_DIR, NEOHAR_DATA_DIR and NEOHAR_RUNTIME_DIR in a fresh temp dir,
# so a test never touches the real ~/.config/neohar, the app data or the runtime sockets.
# CLAUDE_CONFIG_DIR and CODEX_HOME go there too, so a harness or a core that a test starts with the
# environment of this script never reads or writes the real ~/.claude or ~/.codex. HOME stays: the
# caches of cargo and bun are under it.
# Usage: scripts/with-test-env.sh <command> [args...]
set -euo pipefail

# Keep the path short: the runtime dir holds Unix sockets (macOS allows 103 bytes).
root="$(mktemp -d /tmp/nh-test.XXXXXX)"
trap 'rm -rf "$root"' EXIT

export NEOHAR_CONFIG_DIR="$root/config"
export NEOHAR_DATA_DIR="$root/data"
export NEOHAR_RUNTIME_DIR="$root/run"
export CLAUDE_CONFIG_DIR="$root/claude"
export CODEX_HOME="$root/codex"
mkdir -p "$NEOHAR_CONFIG_DIR" "$NEOHAR_DATA_DIR" "$NEOHAR_RUNTIME_DIR" "$CLAUDE_CONFIG_DIR" "$CODEX_HOME"
chmod 700 "$NEOHAR_RUNTIME_DIR"

# On macOS, target/debug/deps collects old object files, and nothing removes them. fseventsd sets
# up the watches of all processes one at a time, and each setup takes more time when the directory
# of the binary has more files: about 1 s with 600 000 files. The watch tests then time out, and
# each first launch of a test binary is slow too. A read of the directory takes seconds then, so
# the check uses its size: on APFS, the size of a directory is 64 + 32 bytes for each entry.
if [[ "$(uname)" == Darwin ]]; then
  deps="${CARGO_TARGET_DIR:-$(cd "$(dirname "$0")/.." && pwd)/target}/debug/deps"
  limit=100000
  size="$(stat -f %z "$deps" 2>/dev/null)" || size=0
  if [[ "$size" -gt $((64 + 32 * limit)) ]]; then
    echo "with-test-env.sh: $deps has more than $limit files. File watches and the first launch" \
      "of each test binary are slow, and the watch tests can time out. Run 'cargo clean'." >&2
  fi
fi

"$@"
