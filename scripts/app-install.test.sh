#!/usr/bin/env bash
# The test of `scripts/app-install.sh`. It uses folders that stand for an app and for
# ~/Applications, under a temporary directory, so it never touches the real ~/Applications.
set -uo pipefail

script="$(cd "$(dirname "$0")" && pwd)/app-install.sh"
work="$(mktemp -d /tmp/nh-install-test.XXXXXX)"
trap 'rm -rf "$work"' EXIT
failures=0

fake_app() { # name build
  mkdir -p "$work/$1/neohar.app/Contents"
  printf '%s' "$2" >"$work/$1/neohar.app/Contents/build"
  echo "$work/$1/neohar.app"
}
install() { # app dir options...
  local app="$1" dir="$2"
  shift 2
  NEOHAR_APP_BUNDLE="$app" NEOHAR_APPLICATIONS_DIR="$dir" bash "$script" --no-verify "$@"
}
check() { # description condition-result
  if [ "$2" = ok ]; then echo "ok   $1"; else echo "FAIL $1"; failures=$((failures + 1)); fi
}
is() { [ "$1" = "$2" ] && echo ok || echo "got '$1', wanted '$2'"; }

# No app: it asks for a build, and makes nothing.
install "$work/nothing/neohar.app" "$work/dest-none" 2>"$work/err" >/dev/null
check "no app: exit 2" "$(is $? 2)"
check "no app: asks for a build" "$(grep -q 'mise run build:app' "$work/err" && echo ok || echo no)"
check "no app: no folder" "$([ ! -e "$work/dest-none" ] && echo ok || echo exists)"

# The first install copies the app.
dest="$work/dest"
install "$(fake_app one 'build 1')" "$dest" >/dev/null 2>&1
check "first install: exit 0" "$(is $? 0)"
check "first install: copied" "$(is "$(cat "$dest/neohar.app/Contents/build")" 'build 1')"

# A second build does not replace it without --yes.
install "$(fake_app two 'build 2')" "$dest" >/dev/null 2>"$work/err"
check "second install: exit 1" "$(is $? 1)"
check "second install: says --yes" "$(grep -q -- '--yes' "$work/err" && echo ok || echo no)"
check "second install: kept" "$(is "$(cat "$dest/neohar.app/Contents/build")" 'build 1')"

# With --yes it replaces it, and the staging folder is gone.
install "$(fake_app two 'build 2')" "$dest" --yes >/dev/null 2>&1
check "replace: exit 0" "$(is $? 0)"
check "replace: new build" "$(is "$(cat "$dest/neohar.app/Contents/build")" 'build 2')"
check "replace: no staging" "$([ ! -e "$dest/.neohar.app.installing" ] && echo ok || echo left)"

# An option that it does not know.
install "$(fake_app three x)" "$work/dest-3" --force >/dev/null 2>"$work/err"
check "unknown option: exit 2" "$(is $? 2)"

[ "$failures" = 0 ] || { echo "$failures checks failed"; exit 1; }
echo "all checks passed"
