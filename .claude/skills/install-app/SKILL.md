---
name: install-app
description: Builds neohar.app from the working tree and installs it in ~/Applications. If neohar is already installed, it updates it - it quits the running app (its panes and agents keep running), replaces the app, opens it again and checks that the panes came back. Then it installs the hooks of Claude Code and Codex again. Use when the user asks to install, reinstall or update the local neohar app or its integrations on this Mac.
argument-hint: "[--no-build] [--force] [--open] [--no-integrations]"
allowed-tools: Bash, Read
---

# Install or update neohar.app

The script `.claude/skills/install-app/scripts/install-app.sh` does the work. It builds the ad-hoc signed app (`mise run build:app`) and installs it with `scripts/app-install.sh`. When the app is already installed, it does these steps in this order:

1. It builds the app first. If the build fails, the installed app does not change.
2. It keeps an installed app that is the same as the build (`diff -rq`), and it does not quit the running app.
3. It quits the running app with a quit event to its PID. This is the same path as "Quit neohar (agents keep running)": the core stores a checkpoint of each pane, and the holders and agents keep running.
4. It replaces the app. `app-install.sh --yes` checks the signature. If the copy stops half way, the old app stays.
5. If the app ran before, the script opens it again. It waits for the core and compares the live panes with the count from before the quit.
6. It installs the hooks of both harnesses again with `nh integration install all`, and prints the action on each file and the notes of the core. The install goes through the core. If no core answers, the script opens the app. If an `nh daemon` serves the session and the app is closed, the install goes through the daemon and the app stays closed. It does this step also when the app is already up to date.
7. It prints the warnings of `nh doctor`.

**A daemon on this Mac.** When an `nh daemon` serves the session, the app is a client of it and starts no core. The update works the same: the quit of the app leaves the daemon and its panes running, and the new app connects to the daemon again. The script never stops the daemon.

The script does not replace the daemon. A daemon keeps the `nh` that started it until `nh daemon --replace`, which is not built yet (P2-02g). The script prints the daemons that it finds, so the user knows which `nh` still runs.

A request to install or update the app or its integrations lets the script quit, replace and open the app. A call of this skill does too. Do not ask again.

## Steps

1. Check the checkout: `git branch --show-current` and `git status --short`. The build uses the working tree as it is. If the branch is not `main`, tell the user and ask before you build. Another session can switch the main checkout to a work-package branch. If the branch is `main` and there are uncommitted changes, continue, and say so in the report.

2. Run the script from the repository root and pass `$ARGUMENTS` through:

   ```bash
   bash .claude/skills/install-app/scripts/install-app.sh $ARGUMENTS
   ```

   On a cold cache, the release build can take more than 10 minutes. Run the script with `run_in_background: true`, then wait for the completion notification. Do not poll.

   Options:
   - `--no-build` installs the bundle from the last `mise run build:app`. To install only the hooks, use it: the app does not change when it is the same as the last build.
   - `--force` replaces an installed app that is the same as the build.
   - `--open` opens the app after the install, also if it was not running before.
   - `--no-integrations` does not install the hooks. Then an app that did not run stays closed, unless you also give `--open`.

3. Report from the output in a few lines:
   - What happened: installed, updated, or already up to date. Include the source branch and commit.
   - For an update of a running app: the live panes before and after, and the pid of the new app.
   - The line `an nh daemon runs: …`, when there is one. Tell the user that the daemon keeps the old `nh` until it is replaced. The hooks (`~/.local/bin/nh`) can point to a newer `nh` than the daemon. A daemon of a development session (`app:dev:client`, `target/dev-run`) is in that list too. It is not the daemon of the installed app.
   - The integrations: the action on each file (`created`, `modified` or `unchanged`) and each `note:` line. `unchanged` means that the file already had the hooks of this nh. After `created` or `modified`, Claude Code uses the new hooks only in a new session. Codex asks again to trust them.
   - Each `warn` or `fail` line from `nh doctor`. A warning that was there before the update is not a new problem. An example is Codex hooks that are not trusted yet: only the user can trust them, in Codex.
   - The ad-hoc signature changes on every build. Thus macOS can ask for its permissions again (for example notifications) after an update.

   After a relaunch, `nh doctor` can say "Claude Code in wN:pN is detected from the process list only. Run `nh integration install claude`". If the integrations step says `unchanged`, the hooks are installed.

   The core does not keep across a relaunch which runs sent hooks (a restored run starts with `hooks: false`). Thus an agent that did nothing since the relaunch has this warning until its next hook, for example the next prompt. Tell the user that. Do not install the hooks again for it.

## When a step fails

The script exits 1 and gives the reason on stderr.

- **The build failed.** Nothing changed. Show the error from the build.
- **neohar did not quit in 30 s.** The installed app did not change. Tell the user to quit neohar from its menu ("Quit neohar (agents keep running)") and run the skill again.
- **The install failed after the quit.** The script opens the old app again. Show the message from `app-install.sh`.
- **The install of the integrations failed.** The app is installed and runs. The script still prints `nh doctor`, then exits 1. Show the error of the core. `~/Applications/neohar.app/Contents/MacOS/nh integration install all --dry-run` shows the changes that the install can make.
- **The core did not answer, or panes are missing after the update.** Run `~/Applications/neohar.app/Contents/MacOS/nh doctor` and `… nh pane list`, and report what they show. The holders keep running when the app quits, so a pane that is not back is a defect in the restore. Report it. Do not try to repair it.
- **A neohar from another bundle runs** (the script gives a warning, for example for `target/release/bundle/macos/neohar.app`). That process holds the API socket, so the installed app cannot start its core. Tell the user, and do not stop that process yourself.

## Rules

- Never `kill` the neohar host, `nh hold` or `nh daemon` processes, never run `nh daemon stop` or `nh daemon --replace` for this skill, and never use "Quit and stop everything". A kill skips the checkpoint, and a stop ends the agents of the user.
- Never set `NEOHAR_STOP_ON_EXIT` for this skill. It stops the panes when the app quits.
- To test the script, set `NEOHAR_APPLICATIONS_DIR` to a folder in the scratchpad, never the real `~/Applications`. A copy that you open without the `NEOHAR_*_DIR` variables changes the real `~/.local/bin/nh` symlink. Thus do not open a test copy. With a destination other than `~/Applications`, the script does not install the hooks and does not open the app for them. The hooks go to the real `~/.claude` and `~/.codex`.
- `nh integration doctor` (the live round trip) is not built yet. It gives the error `unsupported`. Use `nh doctor`.
