#!/usr/bin/env bash
# Run a command for a development session of the app. The three roots of the directories point
# into `target/dev-run`, so a development run never touches the real ~/.config/neohar, the
# real app data or the real sockets. The directories stay between runs, so that a later relaunch
# finds the panes of the run before.
# Usage: scripts/dev-env.sh <command> [args...]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dev="$root/target/dev-run"

export NEOHAR_CONFIG_DIR="$dev/config"
export NEOHAR_DATA_DIR="$dev/data"
# Short: the runtime directory holds Unix sockets (macOS allows 103 bytes).
export NEOHAR_RUNTIME_DIR="/tmp/nhdev-$(id -u)"
export NEOHAR_NH="$root/target/debug/nh"
mkdir -p "$NEOHAR_CONFIG_DIR" "$NEOHAR_DATA_DIR"
mkdir -p -m 700 "$NEOHAR_RUNTIME_DIR"

exec "$@"
