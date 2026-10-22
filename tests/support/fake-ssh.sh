#!/usr/bin/env bash
# A fake `ssh` for the tests of the link. It takes the arguments of
# `ssh <options> <target> -- <command>`, ignores the options, and runs <command> on this Mac,
# with the standard streams of the caller, as the shell of a remote machine would.
#
# The tests give it the remote machine through the environment of the ssh child:
#   REMOTE_<NAME>=<value>   becomes <NAME>=<value> for the command (for example the three
#                           NEOHAR_*_DIR, so the "remote" core has its own directories)
#   FAKE_SSH_LOG=<file>     one line for each argument of the call, then "--"
#   FAKE_SSH_FORWARD_FAIL=<text>  a call with -O (a forward or a cancel) writes <text> and exits 255
#   FAKE_SSH_FAIL=<text>    write <text> to standard error and exit 255, as ssh does for a refusal
#   FAKE_SSH_ASK=<prompt>   run "$SSH_ASKPASS <prompt>" first, as ssh does for a question, and
#   FAKE_SSH_EXPECT=<text>  fail with "Permission denied" unless the answer is <text>
# Nothing else of the local environment reaches the command that names a link of the local app:
# NEOHAR_SOCKET, NEOHAR_SESSION, NEOHAR_MACHINE and the SSH_ASKPASS variables are removed.
set -u

if [ -n "${FAKE_SSH_LOG:-}" ]; then
  {
    for arg in "$@"; do printf '%s\n' "$arg"; done
    echo "--"
    echo "SSH_ASKPASS=${SSH_ASKPASS:-}"
    echo "SSH_ASKPASS_REQUIRE=${SSH_ASKPASS_REQUIRE:-}"
    echo "NEOHAR_SOCKET=${NEOHAR_SOCKET:-}"
    echo "NEOHAR_SESSION=${NEOHAR_SESSION:-}"
    echo "NEOHAR_MACHINE=${NEOHAR_MACHINE:-}"
  } >>"$FAKE_SSH_LOG"
fi

if [ -n "${FAKE_SSH_FAIL:-}" ]; then
  echo "$FAKE_SSH_FAIL" >&2
  exit 255
fi

# `ssh -O forward` and `ssh -O cancel` talk to the control master, which the fake does not have.
for arg in "$@"; do
  if [ "$arg" = "-O" ]; then
    if [ -n "${FAKE_SSH_FORWARD_FAIL:-}" ]; then
      echo "$FAKE_SSH_FORWARD_FAIL" >&2
      exit 255
    fi
    exit 0
  fi
done

if [ -n "${FAKE_SSH_ASK:-}" ]; then
  if [ -z "${SSH_ASKPASS:-}" ] || [ "${SSH_ASKPASS_REQUIRE:-}" != force ]; then
    echo "fake ssh: no SSH_ASKPASS with SSH_ASKPASS_REQUIRE=force" >&2
    exit 255
  fi
  answer="$("$SSH_ASKPASS" "$FAKE_SSH_ASK")" || {
    echo "fake ssh: the question was cancelled" >&2
    exit 255
  }
  if [ "$answer" != "${FAKE_SSH_EXPECT:-}" ]; then
    echo "Permission denied (publickey,password)." >&2
    exit 255
  fi
fi

# The command is everything after the first "--".
while [ $# -gt 0 ] && [ "$1" != "--" ]; do shift; done
[ $# -gt 0 ] || { echo "fake ssh: no -- before the command" >&2; exit 2; }
shift
command="$*"

unset NEOHAR_SOCKET NEOHAR_SESSION NEOHAR_MACHINE SSH_ASKPASS SSH_ASKPASS_REQUIRE
unset FAKE_SSH_LOG FAKE_SSH_FAIL FAKE_SSH_ASK FAKE_SSH_EXPECT
while IFS='=' read -r name value; do
  case "$name" in
    REMOTE_*) export "${name#REMOTE_}=$value" ;;
  esac
done < <(env)

exec sh -c "$command"
