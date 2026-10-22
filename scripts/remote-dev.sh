#!/usr/bin/env bash
# Put a development build of `nh` on the remote machine of NEOHAR_REMOTE_TARGET, under
# ~/.neohar-dev/.
#
# It copies the binary as ~/.neohar-dev/bin/nh (mode 0700) and writes the wrapper
# ~/.neohar-dev/bin/nh-dev. The wrapper sets the three directories (`NEOHAR_*_DIR`) under
# ~/.neohar-dev and /tmp/nh-dev-$UID, because a debug `nh daemon` refuses to start without them.
# It never touches ~/.local/bin/nh, ~/.claude, ~/.codex or /tmp/neohar-$UID.
#
# Usage: NEOHAR_REMOTE_TARGET=<ssh target> scripts/remote-dev.sh [--release] [--no-build] [--bin <path>]
#   --release   build and copy the release binary
#   --no-build  copy the binary that exists (target/<profile>/nh, or --bin)
#   --bin       copy this binary; it implies --no-build
set -euo pipefail

cd "$(dirname "$0")/.."

target="${NEOHAR_REMOTE_TARGET:-}"
if [ -z "$target" ]; then
  echo "remote-dev: set NEOHAR_REMOTE_TARGET to the ssh target of the remote machine" >&2
  exit 2
fi

profile=debug
build=1
bin=""
while [ $# -gt 0 ]; do
  case "$1" in
    --release) profile=release ;;
    --no-build) build=0 ;;
    --bin)
      shift
      bin="${1:?--bin needs a path}"
      build=0
      ;;
    *)
      echo "remote-dev: unknown argument $1" >&2
      exit 2
      ;;
  esac
  shift
done

if [ -z "$bin" ]; then
  if [ "$build" = 1 ]; then
    if [ "$profile" = release ]; then
      mise exec -- cargo build --locked -q -p nh --release
    else
      mise exec -- cargo build --locked -q -p nh
    fi
  fi
  bin="target/$profile/nh"
fi
[ -x "$bin" ] || {
  echo "remote-dev: no binary at $bin" >&2
  exit 1
}

ssh_run() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$target" "$@"; }

local_sum="$(shasum -a 256 "$bin" | cut -d' ' -f1)"
# `~` is for the remote shell: the remote home is not the local home.
remote_sum="$(ssh_run 'shasum -a 256 ~/.neohar-dev/bin/nh 2>/dev/null' | cut -d' ' -f1 || true)"

if [ "$local_sum" != "$remote_sum" ]; then
  # A new file and a rename, so that a running nh keeps its own inode.
  ssh_run 'umask 077 && mkdir -p ~/.neohar-dev/bin && cat > ~/.neohar-dev/bin/nh.tmp && chmod 0700 ~/.neohar-dev/bin/nh.tmp && mv -f ~/.neohar-dev/bin/nh.tmp ~/.neohar-dev/bin/nh' <"$bin"
  echo "remote-dev: copied $bin ($(wc -c <"$bin" | tr -d ' ') bytes) to $target:~/.neohar-dev/bin/nh"
else
  echo "remote-dev: $target already has this nh"
fi

# The wrapper. The quoted heredoc keeps $HOME and $(id -u) for the remote shell.
ssh_run 'umask 077 && cat > ~/.neohar-dev/bin/nh-dev.tmp && chmod 0700 ~/.neohar-dev/bin/nh-dev.tmp && mv -f ~/.neohar-dev/bin/nh-dev.tmp ~/.neohar-dev/bin/nh-dev' <<'WRAPPER'
#!/bin/sh
# Written by scripts/remote-dev.sh of the neohar repository. Do not edit.
dev="$HOME/.neohar-dev"
export NEOHAR_CONFIG_DIR="$dev/config"
export NEOHAR_DATA_DIR="$dev/data"
export NEOHAR_RUNTIME_DIR="/tmp/nh-dev-$(id -u)"
umask 077
mkdir -p "$NEOHAR_CONFIG_DIR" "$NEOHAR_DATA_DIR" "$NEOHAR_RUNTIME_DIR"
exec "$dev/bin/nh" "$@"
WRAPPER
echo "remote-dev: wrote $target:~/.neohar-dev/bin/nh-dev"
