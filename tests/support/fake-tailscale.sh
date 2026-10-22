#!/bin/sh
# A fake `tailscale` for the tests of the phone listener. It never reaches
# tailscaled. It answers `status --json`, `serve status --json`, and the mapping of one HTTPS port
# and its removal, as `phone.publish` and `phone.unpublish` run them.
#
# Its state is in files next to the script, so each test puts a symbolic link to the script into a
# folder of its own and writes the files that it chooses there:
#   status.json  what `tailscale status --json` prints. Without it, the command fails as it does
#                when tailscaled does not run.
#   serve.json   what `tailscale serve status --json` prints. Without it, `{}`. A mapping writes
#                it, and its removal writes `{}`.
#   name         the name of the machine in the mapping. Without it, studio.example.ts.net.
#   fail         when it is there, a change of the mapping writes its text to standard error and
#                exits 1.
#   calls        each run adds one line with its arguments.
dir=$(cd "$(/usr/bin/dirname "$0")" && pwd)
printf '%s\n' "$*" >>"$dir/calls"

case "$*" in
"status --json")
  if [ -f "$dir/status.json" ]; then
    /bin/cat "$dir/status.json"
    exit 0
  fi
  echo "failed to connect to local Tailscale daemon" >&2
  exit 1
  ;;
"serve status --json")
  if [ -f "$dir/serve.json" ]; then
    /bin/cat "$dir/serve.json"
  else
    echo '{}'
  fi
  exit 0
  ;;
esac

if [ "${1:-}" != serve ]; then
  echo "fake tailscale: no such command: $*" >&2
  exit 2
fi
if [ -f "$dir/fail" ]; then
  /bin/cat "$dir/fail" >&2
  exit 1
fi

https=""
target=""
for arg in "$@"; do
  case "$arg" in
  serve | --bg | --yes) ;;
  --https=*) https=${arg#--https=} ;;
  -*)
    echo "fake tailscale: no such flag: $arg" >&2
    exit 2
    ;;
  *) target=$arg ;;
  esac
done
if [ -z "$https" ] || [ -z "$target" ]; then
  echo "fake tailscale: serve needs --https=<port> and a target: $*" >&2
  exit 2
fi

if [ "$target" = off ]; then
  echo '{}' >"$dir/serve.json"
  exit 0
fi
name=studio.example.ts.net
if [ -f "$dir/name" ]; then
  name=$(/bin/cat "$dir/name")
fi
printf '{"TCP":{"%s":{"HTTPS":true}},"Web":{"%s:%s":{"Handlers":{"/":{"Proxy":"%s"}}}}}\n' \
  "$https" "$name" "$https" "$target" >"$dir/serve.json"
exit 0
