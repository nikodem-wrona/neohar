# The zsh startup of a neohar pane. neohar writes this file at each start, so do not edit it.
#
# A login zsh on macOS runs path_helper from /etc/zprofile, and the startup files of the user put
# their directories in front of the PATH. The shims of neohar then come after the real programs,
# and `codex` runs without `--disable daemon_auto_start`. This file is the .zshenv of the ZDOTDIR
# that neohar gives to the shell of a pane. It gives the ZDOTDIR of the user back, reads the
# .zshenv of the user, and puts the shims first in the PATH before each prompt and before each
# command: after all the startup files, and after the precmd hooks of the user (mise, direnv),
# which can change the PATH too.

if [[ -n "${NEOHAR_ZDOTDIR+set}" ]]; then
    ZDOTDIR=$NEOHAR_ZDOTDIR
    unset NEOHAR_ZDOTDIR
else
    unset ZDOTDIR
fi

if [[ -f "${ZDOTDIR:-$HOME}/.zshenv" ]]; then
    builtin source "${ZDOTDIR:-$HOME}/.zshenv"
fi

if [[ -o interactive && -n "${NEOHAR_SHIMS:-}" ]]; then
    _neohar_shims_first() {
        emulate -L zsh
        # The last hook of each list: a hook that the user adds later runs before this one.
        precmd_functions=(${precmd_functions:#_neohar_shims_first} _neohar_shims_first)
        preexec_functions=(${preexec_functions:#_neohar_shims_first} _neohar_shims_first)
        if [[ "${path[1]}" != "$NEOHAR_SHIMS" ]]; then
            path=("$NEOHAR_SHIMS" ${path:#${(b)NEOHAR_SHIMS}})
        fi
    }
    precmd_functions+=(_neohar_shims_first)
    # preexec runs after all the precmd hooks, also at the first prompt.
    preexec_functions+=(_neohar_shims_first)
fi
