#!/usr/bin/env bash
# The script of the /install-app skill. It builds the ad-hoc signed neohar.app from the working tree
# (`mise run build:app`) and installs it in ~/Applications. When neohar is already there, it
# updates it:
#
# 1. It quits the running app with a quit event to its PID. That is the path of "Quit neohar (agents
#    keep running)": the core stores a checkpoint of each pane and ends its lease, and the holders
#    and agents keep running.
# 2. It replaces the app with `scripts/app-install.sh --yes`, which checks the signature and
#    leaves the old app in place if the copy stops half way.
# 3. It opens the new app, waits for its core, and compares the live panes with the count before
#    the quit. The new core reattaches to the holders.
#
# The build runs before the app is touched, so a build that fails changes nothing. An installed app
# that is the same as the build is kept, and the running app is not quit.
#
# Then it installs the hooks of Claude Code and Codex again (`nh integration install all`) and
# prints the warnings of `nh doctor`. The install goes through the core, so the script
# opens the app for it when no core answers. A Mac can also have an `nh daemon` that serves the
# session, and the app is then a client of it. The script uses the core of that daemon
# for the hooks, and does not open the app for them. It never stops the daemon, and it does not
# replace it: a daemon keeps the `nh` that started it until `nh daemon --replace`, and
# the report says so.
#
# Options: --no-build (install the bundle of the last `mise run build:app`), --force (replace an
# installed app that is the same as the build), --open (open the app after the install also when it
# did not run before), --no-integrations (do not install the hooks; a closed app then stays closed
# unless --open).
# Environment: NEOHAR_APPLICATIONS_DIR (the destination, default ~/Applications; app-install.sh
# reads it too). Test the script only with a destination under /tmp. With another destination, the
# script does not install the hooks, because they go to the real ~/.claude and ~/.codex.
# Exit codes: 0 installed, updated or already up to date, and the hooks installed; 1 a step failed;
# 2 wrong use.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$root"
built="$root/target/release/bundle/macos/neohar.app"
dest_dir="${NEOHAR_APPLICATIONS_DIR:-$HOME/Applications}"
dest="$dest_dir/neohar.app"
host_exe="$dest/Contents/MacOS/neohar-desktop"
nh_bin="$dest/Contents/MacOS/nh"

build=1
force=0
open_after=0
integrations=1
for arg in "$@"; do
  case "$arg" in
    --no-build) build=0 ;;
    --force) force=1 ;;
    --open) open_after=1 ;;
    --no-integrations) integrations=0 ;;
    *)
      echo "install-app: unknown option $arg" >&2
      exit 2
      ;;
  esac
done

say() { echo "install-app: $*"; }
fail() {
  echo "install-app: $*" >&2
  exit 1
}

# The PIDs of the processes of this user whose command line is $1, or $1 and arguments.
pids_of() {
  ps -x -ww -o pid=,command= | PREFIX="$1" awk '{
    pid = $1
    sub(/^ *[0-9]+ /, "")
    prefix = ENVIRON["PREFIX"]
    if (prefix != "" && ($0 == prefix || index($0, prefix " ") == 1)) print pid
  }'
}

# The PIDs of neohar hosts that run from a bundle other than the destination.
other_hosts() {
  ps -x -ww -o pid=,command= | DEST="$host_exe" awk '{
    pid = $1
    sub(/^ *[0-9]+ /, "")
    if ($0 ~ /^\/[^ ]*\/neohar\.app\/Contents\/MacOS\/neohar-desktop( |$)/ && index($0, ENVIRON["DEST"]) != 1) print pid
  }'
}

# "<alive> <restoring>": the panes of the core that are alive, and the panes that it still
# restores. "? ?" when no core answers.
pane_counts() {
  "$nh_bin" pane list --json --timeout 5 2>/dev/null | python3 -I -c '
import json, sys
panes = json.load(sys.stdin)["panes"]
alive = sum(1 for p in panes if p.get("liveness", {}).get("state") == "alive")
restoring = sum(1 for p in panes if p.get("restoring"))
print(alive, restoring)
' 2>/dev/null || echo "? ?"
}

# Prints the checks of `nh doctor` that warn or fail, one on each line.
doctor_problems() {
  "$nh_bin" doctor --json 2>/dev/null | python3 -I -c '
import json, sys
for check in json.load(sys.stdin)["checks"]:
    if check["status"] in ("warn", "fail"):
        print("  %s: %s: %s" % (check["status"], check["title"], check["detail"]))
' 2>/dev/null || echo "  nh doctor gave no result"
}

# Installs the hooks of both harnesses at the user scope through the core, and prints the action on
# each file and the notes of the core. Returns 1 when the core refuses or does not answer.
install_integrations() {
  local out
  if ! out="$("$nh_bin" integration install all --json --timeout 30 2>&1)"; then
    say "the install of the integrations failed: $out"
    return 1
  fi
  say "integrations (nh integration install all):"
  OUT="$out" python3 -I -c '
import json, os
result = json.loads(os.environ["OUT"])
for change in result["changes"]:
    backup = " (backup %s)" % change["backup"] if change.get("backup") else ""
    print("  %s: %s%s" % (change["action"], change["path"], backup))
for note in result["notes"]:
    print("  note: %s" % note)
' || echo "  $out"
}

print_doctor() {
  local problems
  problems="$(doctor_problems)"
  if [ -n "$problems" ]; then
    say "nh doctor:"
    echo "$problems"
  else
    say "nh doctor: no warnings"
  fi
}

# Waits until "$@" succeeds, at most $1 seconds. Returns 1 at the time limit.
wait_for() {
  local limit="$1"
  shift
  local deadline=$((SECONDS + limit))
  until "$@"; do
    [ "$SECONDS" -lt "$deadline" ] || return 1
    sleep 0.5
  done
}

host_running() { [ -n "$(pids_of "$host_exe")" ]; }

# The `nh daemon` processes of this user, as "pid path" lines. A daemon of a development session
# is one of them too: the line does not say which session it serves.
daemons() {
  ps -x -ww -o pid=,command= | awk '{
    pid = $1
    sub(/^ *[0-9]+ /, "")
    if ($0 ~ /^\/[^ ]*\/nh daemon( |$)/) { split($0, words, " "); print pid " " words[1] }
  }'
}
host_gone() { ! kill -0 "$1" 2>/dev/null; }
core_answers() { "$nh_bin" session info --timeout 2 >/dev/null 2>&1; }

# True when the core has at least $want_panes live panes and restores none. It keeps the last
# counts in $counts.
restored() {
  counts="$(pane_counts)"
  local alive="${counts% *}" restoring="${counts#* }"
  [ "$alive" != "?" ] && [ "$alive" -ge "$want_panes" ] && [ "$restoring" = 0 ]
}

# Opens the installed app and waits for its core. $1 is the count of live panes to wait for.
open_and_check() {
  want_panes="$1"
  open "$dest"
  wait_for 60 host_running || fail "neohar did not start from $dest in 60 s."
  # The first launch of a new ad-hoc signed binary is slow: the system checks it first.
  wait_for 90 core_answers || fail "the core of neohar did not answer in 90 s. Run: $nh_bin doctor"
  if [ "$want_panes" -gt 0 ]; then
    if wait_for 60 restored; then
      say "panes: $want_panes live before the quit, ${counts% *} live now"
    else
      say "panes: $want_panes live before the quit, but now ${counts% *} live and ${counts#* } restoring"
    fi
  fi
  say "neohar runs from $dest (pid $(pids_of "$host_exe" | head -1))"
}

[ "$(uname -s)" = Darwin ] || {
  echo "install-app: neohar.app is built for macOS only." >&2
  exit 2
}
command -v mise >/dev/null || {
  echo "install-app: mise is not on PATH. Run 'mise install' first." >&2
  exit 2
}
if [ "$integrations" = 1 ] && [ "$dest_dir" != "$HOME/Applications" ]; then
  integrations=0
  say "the destination is $dest_dir, not ~/Applications, so the hooks are not installed"
fi

branch="$(git branch --show-current)"
commit="$(git rev-parse --short HEAD)"
dirty=""
[ -z "$(git status --porcelain)" ] || dirty=", with uncommitted changes"
say "source: ${branch:-detached HEAD} at $commit$dirty"

if [ "$build" = 1 ]; then
  say "building neohar.app (mise run build:app)"
  mise run build:app || fail "the build failed. The installed app did not change."
fi
[ -d "$built" ] || fail "there is no app at $built. Run without --no-build."

up_to_date=0
if [ -d "$dest" ]; then
  mode=updated
  if [ "$force" = 0 ] && diff -rq "$built" "$dest" >/dev/null 2>&1; then
    say "$dest is already up to date with the build. The app did not change."
    up_to_date=1
  fi
else
  mode=installed
fi

others="$(other_hosts | paste -sd ' ' -)"
[ -z "$others" ] || say "warning: a neohar from another bundle runs (pid $others). It holds the API socket, so the installed app cannot start its core."

daemon_lines="$(daemons)"
if [ -n "$daemon_lines" ]; then
  say "an nh daemon runs: $(echo "$daemon_lines" | paste -sd ';' - | sed 's/;/; /g'). The install does not stop it, and it keeps the nh that started it until 'nh daemon --replace'."
fi

was_running=0
panes_before=0
if [ "$up_to_date" = 0 ]; then
  holders_before="$(pids_of "$nh_bin hold" | wc -l | tr -d ' ')"
  pid="$(pids_of "$host_exe" | head -1)"
  if [ -n "$pid" ]; then
    was_running=1
    counts="$(pane_counts)"
    [ "${counts% *}" = "?" ] || panes_before="${counts% *}"
    say "quitting neohar (pid $pid). Its $panes_before live panes and $holders_before holders keep running."
    osascript -l JavaScript -e "ObjC.import('AppKit'); var app = \$.NSRunningApplication.runningApplicationWithProcessIdentifier($pid); app.isNil() ? 'gone' : String(app.terminate)" >/dev/null ||
      fail "the quit event to neohar (pid $pid) failed. The installed app did not change."
    wait_for 30 host_gone "$pid" ||
      fail "neohar (pid $pid) did not quit in 30 s. The installed app did not change. Quit it from its menu and run again."
  elif [ "$holders_before" -gt 0 ]; then
    say "neohar does not run, but $holders_before of its holders do. The next launch brings their panes back."
  fi

  if ! NEOHAR_APP_BUNDLE="$built" bash "$root/scripts/app-install.sh" --yes; then
    if [ "$was_running" = 1 ] && [ -d "$dest" ]; then
      say "the install failed. Opening the old app again."
      open "$dest"
    fi
    fail "the install failed."
  fi
  say "$mode $dest from $commit"
fi

# The hooks go through the core, so the app must run for them.
if [ "$was_running" = 1 ]; then
  open_and_check "$panes_before"
elif host_running; then
  wait_for 90 core_answers || fail "the core of neohar did not answer in 90 s. Run: $nh_bin doctor"
  say "neohar runs from $dest (pid $(pids_of "$host_exe" | head -1))"
elif [ "$open_after" = 0 ] && [ "$integrations" = 1 ] && core_answers; then
  # A daemon serves the session. Its core takes the install of the hooks.
  say "neohar is closed, and a daemon serves the session. The hooks are installed through the daemon, and the app stays closed. Open it with: open \"$dest\""
elif [ "$open_after" = 1 ] || [ "$integrations" = 1 ]; then
  [ "$open_after" = 1 ] || say "opening neohar: the hooks are installed through its core"
  open_and_check 0
else
  say "neohar did not run before, so it stays closed. Open it with: open \"$dest\""
  exit 0
fi

status=0
if [ "$integrations" = 1 ]; then
  install_integrations || status=1
fi
print_doctor
exit "$status"
