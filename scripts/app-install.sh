#!/usr/bin/env bash
# `mise run app:install`: copies the ad-hoc signed neohar.app (made by `mise run build:app`) to
# ~/Applications.
#
# It never replaces an app that is already there unless you pass --yes: the app may be running
# panes, and an install over it is a choice of the person. The copy keeps the signature (`ditto`),
# and the script checks it with `codesign` before and after the copy. Ad-hoc signatures change on
# every build, so macOS can ask for permissions again after an install.
#
# Options: --yes (replace an installed app), --no-verify (skip `codesign`; for the tests of this
# script, which copy a folder that is not a real app).
# Environment: NEOHAR_APPLICATIONS_DIR (the destination, default ~/Applications) and
# NEOHAR_APP_BUNDLE (the app to install, default target/release/bundle/macos/neohar.app).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
source_app="${NEOHAR_APP_BUNDLE:-$root/target/release/bundle/macos/neohar.app}"
dest_dir="${NEOHAR_APPLICATIONS_DIR:-$HOME/Applications}"
dest_app="$dest_dir/neohar.app"
replace=0
verify=1
for arg in "$@"; do
  case "$arg" in
    --yes) replace=1 ;;
    --no-verify) verify=0 ;;
    *)
      echo "app:install: unknown option $arg" >&2
      exit 2
      ;;
  esac
done

if [ ! -d "$source_app" ]; then
  echo "app:install: there is no app at $source_app. Run 'mise run build:app' first." >&2
  exit 2
fi
if [ "$verify" = 1 ]; then
  codesign --verify --deep --strict "$source_app" || {
    echo "app:install: the signature of $source_app is not valid." >&2
    exit 2
  }
fi
if [ -e "$dest_app" ] && [ "$replace" != 1 ]; then
  echo "app:install: $dest_app is already installed. Quit neohar, and run again with --yes to replace it." >&2
  exit 1
fi

mkdir -p "$dest_dir"
# A copy next to the destination, and then a rename: an install that stops half way leaves the old
# app in place.
staging="$dest_dir/.neohar.app.installing"
rm -rf "$staging"
if [ "$(uname -s)" = Darwin ]; then
  ditto "$source_app" "$staging"
else
  # Linux has no `ditto` and no app bundle. The tests of this script copy a plain folder there.
  cp -a "$source_app" "$staging"
fi
if [ "$verify" = 1 ]; then
  codesign --verify --deep --strict "$staging" || {
    rm -rf "$staging"
    echo "app:install: the copy has a signature that is not valid." >&2
    exit 1
  }
fi
rm -rf "$dest_app"
mv "$staging" "$dest_app"
echo "Installed $dest_app"
