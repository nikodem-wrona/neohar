# VT corpus

Recorded terminal journals of real programs. The snapshot check (`tools/nh-snapcheck`) replays them through the VT engine to check that a snapshot restores the screen exactly. A recording is the raw output of a program under a real holder, with the resizes and the cut points that the holder wrote. The synthetic seeds of `tools/nh-fixture` are not stored here. They are generated at test time.

## Files

For each recording `<name>`:

| File | What it is |
|---|---|
| `<name>.journal` | The recording. It is committed. |
| `<name>.steps` | The script that drove the program: the keys that were sent and the text that was awaited. |
| `<name>.setup.sh` | A shell script that makes the files the program needs (a source file, a repository, a config). |

`man` has no setup script. The setup scripts also write launcher scripts (`launch.sh`) for `top` and `htop`.

## The journal format

A `.journal` file is the framing of the holder (`nh_proto::framing`): a series of messages, each a `u32` little-endian length and a `postcard` body of one `HolderEvent`. The file holds, in the order and with the offsets that the holder sent them:

- a synthetic `Marker::Resize` at offset 0 with the size at the start (the holder writes no marker for its first size),
- every `Output` event of the `Pty` stream,
- every `Marker` event: `Resize` (a `resize` step), `InputAck` (a `send` step) and `CutPoint`.

The offsets are those of the holder, so output events follow each other without a gap. Read a file with `nh_record::journal::read_journal`. Look at one with `nh-record --dump <file>`: it prints one line per event, with escapes shown as `\e`, `\r` and so on.

The programs ran with no terminal emulator behind the PTY, so nobody answered their terminal queries (DA, DSR, OSC 10 and 11). The holder answers no query while a client is attached. The queries are in the journals, and the replies are not. A program that waits for a reply, like vim, waits for its time-out and goes on.

## The step script

A script is a text file with one step per line (not YAML, because YAML needs a parser crate that the workspace does not have). The full format is in the doc comment of `tools/nh-record/src/script.rs`. In short:

```text
# a comment
send "text with \r \n \t \e \xNN escapes"   # input for the program
expect "text"                               # wait until the output has this text
wait 300                                    # keep reading for 300 ms (for programs that redraw on a timer)
resize 80 24                                # change the PTY size; the journal gets a marker
expect_exit                                 # wait until the program exits
```

`expect` searches the raw output, escape sequences included, and starts after the end of the previous match. It fails after 15 s. The scripts wait for what a program prints, and use `wait` only for `top`, where each refresh looks different.

## How to record

Run every command from the repository root. Build the holder and the tool first:

```bash
mise exec -- cargo build -p nh -p nh-record
```

`nh-record` finds `nh` next to its own executable (`--nh <path>` and `NEOHAR_NH` override that). It makes a scratch directory `/tmp/nhr-<pid>-<n>/` and runs the setup script there. Then it starts a holder, and the holder runs the program. At the end, it removes the directory.

The program gets a minimal environment: `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, `TERM=xterm-256color`, `LANG=en_US.UTF-8`, and `HOME`, `TMPDIR` and the `NEOHAR_*` directories inside the scratch directory. Nothing of the user's environment gets in. In `--env` values and arguments, `{work}`, `{home}`, `{tmp}` and `{scratch}` stand for the scratch directories. The default size is 100x30.

The holder and the program are stopped on every way out of the tool (the end, an error, a panic, Ctrl-C and SIGTERM). If the tool gets SIGKILL, check with `pgrep -fl 'nh hold'`.

Re-record one program with its command. Each one writes `tests/vt-corpus/<name>.journal`.

```bash
# vim: edit a C file, move, search, edit, split windows, three resizes, mouse, paste
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/vim.journal --script tests/vt-corpus/vim.steps --setup tests/vt-corpus/vim.setup.sh -- /usr/bin/vim -u vimrc -N -n -i NONE sample.c

# less: 3,000 lines with colours and wide characters, pages, search, one resize, chop long lines
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/less.journal --script tests/vt-corpus/less.steps --setup tests/vt-corpus/less.setup.sh --env LESS=-R --env LESSHISTFILE=- -- /usr/bin/less big.txt

# top: the interactive macOS top on one busy process, refreshes, help screen, sort key
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/top.journal --script tests/vt-corpus/top.steps --setup tests/vt-corpus/top.setup.sh -- ./launch.sh

# htop: four processes of ours, sort, tree, filter, help, setup screens, mouse, two resizes
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/htop.journal --script tests/vt-corpus/htop.steps --setup tests/vt-corpus/htop.setup.sh -- ./launch.sh

# man: man ls, bold and underline, pages, search, one resize
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/man.journal --script tests/vt-corpus/man.steps --env PAGER=less --env MANPAGER=less -- /usr/bin/man ls

# bash: prompts, colours, scrolling, completion, line editing, paste, history search, resizes
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/shell.journal --script tests/vt-corpus/shell.steps --setup tests/vt-corpus/shell.setup.sh --env 'PS1=\[\e[1;32m\]demo\[\e[0m\] \[\e[1;34m\]\W\[\e[0m\] \$ ' --env 'PATH={work}/bin:/usr/bin:/bin:/usr/sbin:/sbin' --env CLICOLOR=1 -- /opt/homebrew/bin/bash --noprofile --norc -i

# zsh: right prompt, completion menu, EOL mark, autocd, history search, paste, resizes
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/zsh.journal --script tests/vt-corpus/zsh.steps --setup tests/vt-corpus/zsh.setup.sh --env 'ZDOTDIR={home}' --env 'PATH={work}/bin:/usr/bin:/bin:/usr/sbin:/sbin' --env CLICOLOR=1 -- /bin/zsh -d -i

# git log: graph, decorations and stat, paged on the main screen
mise exec -- cargo run -q -p nh-record -- --out tests/vt-corpus/git-log.journal --script tests/vt-corpus/git-log.steps --setup tests/vt-corpus/git-log.setup.sh --env GIT_PAGER=less --env LESS=FRX -- /usr/bin/git -C repo log --graph --decorate --stat
```

A recording is not byte for byte the same each time. The timing of the program changes how its output splits into chunks, and `top` and `htop` print numbers of the machine. A new recording is as good as the old one if the steps pass.

### git log

`git-log.setup.sh` makes a repository with 62 commits on five branches, four merges, three tags, and fictional authors on `example.invalid`. It uses fixed dates. It passes `-c user.name=... -c user.email=...` to every command that makes a commit or a tag. Then git does not look up a user name or a host name of the machine. The recording ran with `git version 2.54.0 (Apple Git-157)`. A search of the journal for the user name, the host name and the home path found nothing.

## What the recordings hold

All of them were made on macOS 26.6.2 (arm64) on 2026-10-07 at 100x30 with `TERM=xterm-256color`. The sizes are of the `.journal` files. "Output" is the number of PTY bytes in them.

| Recording | Program | Size | Output | Cut points | Resizes | What it exercises |
|---|---|---|---|---|---|---|
| `vim` | `/usr/bin/vim` 9.1 (patches 1-1752) | 47,333 B | 44,850 B | 112 | 3 | Alternate screen, scroll regions with insert and delete line, 256-colour syntax SGR, wide characters, the ruler and the command line, window splits and status lines, mouse modes 1002 and 1006, bracketed paste, focus events, `CSI 22/23 t` title stack |
| `less` | `/usr/bin/less` 668 | 40,611 B | 38,813 B | 85 | 1 | Alternate screen, reverse index scrolling, SGR with 16, 256 and RGB colours, wide and combining characters, search highlight, soft wrap and chop |
| `top` | macOS `/usr/bin/top` (interactive) | 5,645 B | 5,170 B | 23 | 0 | Cell-by-cell updates with `CUP`, `VPA`, `CHA` and `DCH`, reverse-video header, help screen, prompt line |
| `htop` | `/opt/homebrew/bin/htop` 3.5.2 | 28,694 B | 26,812 B | 92 | 2 | Alternate screen, bars of CPU meters with colours, `ECH`, `DCH`, panels, tree characters, mouse modes 1000 and 1006, resize |
| `man` | macOS `/usr/bin/man` with `less` | 19,053 B | 18,183 B | 43 | 1 | Bold and underline (SGR 1 and 4), pager prompts, search, resize |
| `shell` | `/opt/homebrew/bin/bash` 5.3.15 | 13,663 B | 11,716 B | 97 | 2 | Main screen only, scrolling into history, prompt colours, bracketed paste mode, completion lists and bell, readline editing and search, `clear` |
| `zsh` | `/bin/zsh` 5.9 | 18,935 B | 14,635 B | 236 | 2 | Main screen only, right prompt with cursor moves, reverse-video EOL mark, completion menu, incremental search, paste highlight, resize |
| `git-log` | `/usr/bin/git` 2.54.0 (Apple Git-157) with `less -FRX` | 16,823 B | 16,013 B | 38 | 1 | Colours, graph characters, scrolling on the main screen (`X`) |

The total of the recorded journals is 190,757 B.

The bash recording uses Homebrew bash 5.3.15 (`/opt/homebrew/bin/bash`), because its readline turns bracketed paste on. `/bin/bash` is 3.2.57. It also works.

The `top` and `htop` recordings use `-pid` and `-p` on processes that the setup starts: a busy `perl` loop, and for htop also a light `perl` loop and a `sleep`. Without these options, both programs list the processes of the machine, with their names, users and command lines. htop shows a fifth row: `htop` with a pid that is not a real pid. That is how htop 3.5.2 on macOS answers `-p`.

## Privacy checks

The programs run with the environment that `nh-record` builds, in a scratch directory under `/tmp`. A recording must not hold these items: a user name, a host name, a home path, a scratch path, an e-mail address, or a process of the machine. After each recording, search the journal:

```bash
grep -a -c -i -E "nhr-|/tmp/|/private|/var/folders|$(id -un)|$(hostname -s)|/Users|homebrew" tests/vt-corpus/<name>.journal
```

The count must be 0 for every journal. The recordings in this directory were checked that way and also with the escape sequences removed (`perl -0777 -pe 's/\e\[[0-9;?<>=]*[ -\/]*[\@-~]//g'`), and with `strings -a`. If a recording holds such text, change the setup or the steps (an explicit `PS1`, a scratch directory, `-pid`) and record again. Do not edit a journal.
