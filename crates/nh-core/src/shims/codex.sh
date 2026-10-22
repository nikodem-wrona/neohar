#!/bin/sh
# The codex shim of neohar. neohar writes this file at each start, so do not edit it.
#
# It runs the next `codex` on the PATH after its own place, with `--disable daemon_auto_start` and
# `--no-daemon`. Then the Codex of a pane runs its own app-server under the environment of the pane,
# and its hooks carry the token of the pane. The first flag only stops Codex from starting the shared
# app-server daemon: Codex 0.163 attaches to a daemon that runs already, and then the hooks run in the
# environment of the daemon. The arguments of the user are passed on as they are. Three cases
# pass everything on without the flags: CODEX_NEOHAR_SHIM=0, an argument that names the feature, and
# `--no-daemon`. A Codex that does not know `--no-daemon`, and the commands that need the shared
# daemon (`agents`, `queue`, `remote-control`, `app-server`, `--remote`), get the first flag only.
#
# The search starts after the place of the shim in the PATH. A second shim farther on (neohar in a
# pane of neohar) is then run once, and it looks after its own place: the chain ends at the real
# program.

self_text=$(dirname "$0")
self_phys=$(cd "$self_text" 2>/dev/null && pwd -P)

# When the shim was found through the PATH, the entries before it and it are not candidates.
after_self=0
case ":$PATH:" in
    *":$self_text:"*) after_self=1 ;;
esac

real=
passed=0
old_ifs=$IFS
IFS=:
for dir in $PATH; do
    [ -n "$dir" ] || continue
    if [ "$after_self" = 1 ] && [ "$passed" = 0 ]; then
        [ "$dir" = "$self_text" ] && passed=1
        continue
    fi
    here=$(cd "$dir" 2>/dev/null && pwd -P) || continue
    [ "$here" = "$self_phys" ] && continue
    if [ -f "$dir/codex" ] && [ -x "$dir/codex" ]; then
        real="$dir/codex"
        break
    fi
done
IFS=$old_ifs

if [ -z "$real" ]; then
    echo "codex: command not found" >&2
    exit 127
fi

if [ "${CODEX_NEOHAR_SHIM:-1}" = "0" ]; then
    exec "$real" "$@"
fi

# A command of the daemon can come after options (`codex -m x agents`), so every argument counts.
# An option value with such a name only costs the second flag.
daemon=0
for arg in "$@"; do
    case $arg in
        *daemon_auto_start* | --no-daemon) exec "$real" "$@" ;;
        agents | queue | remote-control | app-server | --remote | --remote=*) daemon=1 ;;
    esac
done

if [ "$daemon" = 0 ] && "$real" --no-daemon --version </dev/null >/dev/null 2>&1; then
    exec "$real" --disable daemon_auto_start --no-daemon "$@"
fi
exec "$real" --disable daemon_auto_start "$@"
