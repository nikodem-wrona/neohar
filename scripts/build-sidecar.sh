#!/usr/bin/env bash
# Build `nh` in release for the host triple and copy it to the path that Tauri bundles as the
# sidecar: apps/desktop/src-tauri/binaries/nh-<target-triple>. The bundler puts it in
# Contents/MacOS/nh. The directory is git-ignored.
# Usage: scripts/build-sidecar.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

triple="$(rustc -vV | sed -n 's/^host: //p')"
if [ -z "$triple" ]; then
  echo "build-sidecar: cannot read the host triple from rustc" >&2
  exit 1
fi

cargo build --release --locked -p nh
mkdir -p apps/desktop/src-tauri/binaries
cp target/release/nh "apps/desktop/src-tauri/binaries/nh-$triple"
echo "sidecar: apps/desktop/src-tauri/binaries/nh-$triple"
