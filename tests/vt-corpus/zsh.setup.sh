#!/bin/sh
# Setup for zsh.steps: the same files and command as shell.setup.sh, and a .zshrc in the scratch
# home (the recording sets ZDOTDIR to it) with a simple prompt, a right prompt, compinit and a
# completion menu. zsh runs with -d, so that the global files of the machine are not read.
#   quarterly-report-final.txt   the only file that starts with "quar"
#   notes-a.txt, notes-b.txt     two files that start with "notes-" (an ambiguous prefix)
#   zzprobe-greet                the only command that starts with "zzprobe"
set -e

echo 'Q3 report: all numbers are made up' > quarterly-report-final.txt
echo 'first note' > notes-a.txt
echo 'second note' > notes-b.txt

mkdir -p bin
cat > bin/zzprobe-greet <<'EOF'
#!/bin/sh
echo "hello from zzprobe-greet"
EOF
chmod +x bin/zzprobe-greet

cat > "$HOME/.zshrc" <<'EOF'
PROMPT='%F{green}%Bdemo%b%f %F{blue}%1~%f %# '
RPROMPT='%F{yellow}[zsh]%f'
HISTFILE="$HOME/.zsh_history"
HISTSIZE=1000
SAVEHIST=1000
setopt AUTO_CD INTERACTIVE_COMMENTS HIST_IGNORE_DUPS
bindkey -e
autoload -Uz compinit
compinit -u -d "$HOME/.zcompdump"
zstyle ':completion:*' menu select
zstyle ':completion:*' list-colors 'di=34:ex=32'
EOF
