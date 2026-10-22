#!/usr/bin/env bash
# Start `nh daemon` for the session of a development run, and open the app as its client through
# link.sock. Run it through `scripts/dev-env.sh` (`mise run app:dev:client`), so
# that the three directories are the ones of the development run.
#
# The daemon runs in the background, and its standard input is a pipe of this script. A quit of
# the app leaves the daemon and its panes running. Then, on a terminal, Enter opens the app again as
# a client of the same daemon, and q stops the daemon. Without a terminal, the script stops the
# daemon after the first quit. The daemon stops its panes when its standard input closes.
#
# NEOHAR_APP_BIN runs that build of the app instead of `cargo tauri dev`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
: "${NEOHAR_RUNTIME_DIR:?run this script through scripts/dev-env.sh}"
export NEOHAR_SESSION="${NEOHAR_SESSION:-dev}"

cargo build -p nh --locked

log="$NEOHAR_RUNTIME_DIR/daemon-$NEOHAR_SESSION.log"
fifo="$NEOHAR_RUNTIME_DIR/daemon-$$.stdin"
rm -f "$fifo"
mkfifo -m 600 "$fifo"
# The script holds the only writer of the pipe on fd 3. No child gets it (`3>&-`), so the daemon
# reads the end of its input when the script ends.
exec 3<>"$fifo"
"$root/target/debug/nh" daemon --session "$NEOHAR_SESSION" <"$fifo" >"$log" 2>&1 3>&- &
daemon=$!

stop_daemon() {
  exec 3>&-
  rm -f "$fifo"
  echo "app-dev-client: the daemon stops its panes and ends" >&2
  wait "$daemon" 2>/dev/null || true
}
trap stop_daemon EXIT

# The daemon prints one line of JSON when its sockets are bound.
for _ in $(seq 1 200); do
  if grep -q '"link_socket"' "$log" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$daemon" 2>/dev/null; then
    echo "app-dev-client: the daemon did not start:" >&2
    cat "$log" >&2
    exit 1
  fi
  sleep 0.1
done
echo "app-dev-client: nh daemon (pid $daemon) serves the session $NEOHAR_SESSION: $(head -n 1 "$log")" >&2

while true; do
  if [[ -n "${NEOHAR_APP_BIN:-}" ]]; then
    "$NEOHAR_APP_BIN" 3>&- || true
  else
    (cd apps/desktop && cargo tauri dev 3>&-) || true
  fi
  answer=q
  if [[ -t 0 ]]; then
    read -r -p "neohar quit, and the daemon and its panes keep running. Enter opens the app again, q stops the daemon: " answer || answer=q
  fi
  if [[ "$answer" == q ]]; then
    break
  fi
done
