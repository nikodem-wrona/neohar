# Changelog

This file records the notable changes to neohar.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), with the breaking
changes first. neohar uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
`mise run release` writes each section from the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) since the last release. Do
not edit a released section. A note under Unreleased goes to the top of the next release.

## [Unreleased]

## [2.0.0] - 2026-10-25

This release is the first with a DMG for Apple silicon. The DMG has an ad-hoc signature, and Apple
did not notarize it. Drag neohar to the Applications folder. Then run
`xattr -dr com.apple.quarantine /Applications/neohar.app`, because without it macOS refuses to open
the app. [Install the app](https://github.com/nikodem-wrona/neohar/blob/main/website/docs/guides/install.md)
tells more.

Read this before you update. This release closes the findings of a security review, and some of
the fixes change what you do:

- neohar runs git in a repository only after you trust it. Click **Trust** in the Changes tab or
  the Git view once for each repository, or run `nh git trust <path>`. neohar asks again when a
  setting or a hook of the repository that can start a program changes.
- "Allow always" shows only when neohar can make a rule that covers the request exactly, and the
  button shows the rule. A notification and the tray show Allow only for a command of low or
  medium risk that the title shows in full. For other requests, open the card.
- From inside a neohar pane, neohar refuses these calls: an answer to a request, a change of the
  policy, `nh git trust`, `nh config set` for the keys of `approvals`, `policy`, `phone` and
  `machines`, and input to a pane that waits for an answer. Use the app, or a terminal that is not
  a neohar pane.
- `nh --socket` refuses a socket in a folder that other users can write, for example directly in
  `/tmp`.
- neohar removes a paired phone that did not connect for 30 days. Pair it again.
- A `neohar://pane/…` link shows the app but does not select the pane.

### Breaking changes

- remove the task workspaces. the methods task.*, favorite.*, worktree.list and fs.dirs give method_not_found. The projections tasks, favorites and changes:<task>, the events task.* and favorite.*, the error codes agent_failed, setup_failed and no_free_ports, and the handle k<n> are gone. The fields task of AgentValue and agent.start, task and base of ChangesHead, commits and merge_base of ChangeTotals, and uncommitted of ChangeRow are gone. The keys tasks.* of config.toml have no effect, and the migration 16 removes the tasks and the favorite folders from the database. The worktrees and the branches of the tasks stay on the disk: remove them with git worktree remove and git branch -d. ([89d5edb](https://github.com/nikodem-wrona/neohar/commit/89d5edb2d10beb70ad02c99c2aebdea1aee4fe4c))
- **api:** remove the method window.pick_folder. the method window.pick_folder gives method_not_found. The types WindowPickFolder and FolderPicked are gone. Only a client with the scope ui could call the method. ([0d22ecc](https://github.com/nikodem-wrona/neohar/commit/0d22eccedf11a0e8ab721822417229c499ab1818))
- run git only in a repository that the person trusts. the `git.*` methods and the `nh git` commands answer permission_denied in a repository that nobody trusted. Trust each repository once: click Trust in the Changes tab or the Git view, or run `nh git trust <path>`. ([f4e2fe7](https://github.com/nikodem-wrona/neohar/commit/f4e2fe7b1f08167a3eb9278a8718a69cf01152f2))

### Added

- add the Git view ([#3](https://github.com/nikodem-wrona/neohar/pull/3))
- attach a DMG of the app to each GitHub release ([#7](https://github.com/nikodem-wrona/neohar/pull/7))

### Fixed

- **deps:** update ctutils and clipboard-win in Cargo.lock ([90b2be4](https://github.com/nikodem-wrona/neohar/commit/90b2be43e28d9cae454b04e7aff8fa4a99ef8f55))
- **core:** give each change of a harness manifest one event ([#2](https://github.com/nikodem-wrona/neohar/pull/2))
- **deps:** update crypto-bigint for RUSTSEC-2026-0387 ([#4](https://github.com/nikodem-wrona/neohar/pull/4))
- type an answer only into the dialog of its own request ([1f76dd4](https://github.com/nikodem-wrona/neohar/commit/1f76dd4211ea0772ff1cf586117ec5b7c553fa63))
- **core:** let a pane token resume only the hooks of its own pane ([cad0274](https://github.com/nikodem-wrona/neohar/commit/cad0274113540b97fe5ae44adddfef98e81c50e8))
- **core:** refuse approval changes from a client inside a pane ([75bcd1c](https://github.com/nikodem-wrona/neohar/commit/75bcd1c8651c68784e1d2288c17c83c5e952554e))
- **store:** prune ended interactions and redact them when they end ([4693f8f](https://github.com/nikodem-wrona/neohar/commit/4693f8f8d770663f554945fa5e7db2b283efbb46))
- **rich:** score git options, links and invisible characters ([855e8e7](https://github.com/nikodem-wrona/neohar/commit/855e8e76213fa133c6674adbf2858953779b9e40))
- **core:** match allow rules only on plain command lines ([0ab99d6](https://github.com/nikodem-wrona/neohar/commit/0ab99d6cf8c18e344af17dba66a281c4a5e7b65c))
- **core:** give "Allow always" only for a rule that covers the request ([fcce190](https://github.com/nikodem-wrona/neohar/commit/fcce190952510c22a6a140cd4d3e00d68f5d8739))
- **core:** offer Allow outside the card only for a whole command ([3888901](https://github.com/nikodem-wrona/neohar/commit/38889018ada29e75ed8a2f79d8b6d84644b5a901))
- **core:** show invisible characters in the names of the Files tab ([8aeec69](https://github.com/nikodem-wrona/neohar/commit/8aeec6951f6efa33216e6bd75a58b10b317c28ed))
- **remote:** clean the HTML of a remote core again on the client ([180d735](https://github.com/nikodem-wrona/neohar/commit/180d735ad74cf47f7be4507ff9b1f740563d43e8))
- **desktop:** open a link of rich text only after a click ([6ced888](https://github.com/nikodem-wrona/neohar/commit/6ced888046fbd1c4b6aaa7f6bbc1594eac89bb24))
- **remote:** gate the clipboard writes and focus requests of a core ([e95f3fe](https://github.com/nikodem-wrona/neohar/commit/e95f3febeb927bd4c14a9afeea6cab03ed584a76))
- **remote:** bound what a remote core can make the Mac keep ([58d5fc0](https://github.com/nikodem-wrona/neohar/commit/58d5fc010438763832eb0909e49e94df67d62df3))
- **desktop:** label the ssh questions of a server and cap them ([dd1cc43](https://github.com/nikodem-wrona/neohar/commit/dd1cc437a978ce46bd97a40f2a5d45e1ce43fd46))
- **desktop:** keep local files and ui.editor out of remote windows ([97c8409](https://github.com/nikodem-wrona/neohar/commit/97c840939856599a28b48b1e1736ce3d8aec727c))
- **term:** refuse to paste a path with a control character ([be3b0e1](https://github.com/nikodem-wrona/neohar/commit/be3b0e121bef33a1584bdefc802b2705aecb1941))
- **desktop:** stop a deep link from moving the focus to a pane ([10bec84](https://github.com/nikodem-wrona/neohar/commit/10bec84a6e1708799bd1a8bbace226a3aac49857))
- **desktop:** serve a blob in a sandbox, and a document as an attachment ([bc18f87](https://github.com/nikodem-wrona/neohar/commit/bc18f8783ddf4fed661698f7ca5d46161c537c1f))
- **remote:** give each quick copy of the person its own clipboard write ([e3870a4](https://github.com/nikodem-wrona/neohar/commit/e3870a474ce2c0ef2a87165a7777191b21846773))
- **remote:** send every field of a tray line and check cut labels ([264cf85](https://github.com/nikodem-wrona/neohar/commit/264cf85fde44c5c10b452f0698c713107a413f58))
- refuse a socket of another user in each client of the core ([c4200ed](https://github.com/nikodem-wrona/neohar/commit/c4200ed5a8746c34840e627e78ec4bbedc835296))
- **desktop:** skip the single-instance socket of another user ([2f7935a](https://github.com/nikodem-wrona/neohar/commit/2f7935a78bd1111312ae093ff5fa88652f919adf))
- **phone:** remove the tailscale serve mapping when the listener stops ([d8545ad](https://github.com/nikodem-wrona/neohar/commit/d8545adcebd4ebc57134d04ca67f7d04a9ada58b))
- **phone:** limit the connections that unpaired clients can hold ([3650249](https://github.com/nikodem-wrona/neohar/commit/365024914ae350e31b6411e6cb827898f33667fd))
- **phone:** refuse every request while a Funnel is on the HTTPS port ([b4e3e9a](https://github.com/nikodem-wrona/neohar/commit/b4e3e9ae35abb828f8717c66d6fccef2d9bff56c))
- **phone:** keep the terminal title out of the text of a push ([0fbd865](https://github.com/nikodem-wrona/neohar/commit/0fbd865242be4dd087c93ea7b4a597a1a6d7b80c))
- **phone:** count wrong pairing codes for each client ([3eff737](https://github.com/nikodem-wrona/neohar/commit/3eff737527ba1088df5401107679bb03a2288ff0))
- **phone:** remove a paired phone after 30 days without a use ([ad0cc7a](https://github.com/nikodem-wrona/neohar/commit/ad0cc7aa232b852cb7277b7faed2c44cc9897cc4))
- **remote:** let the core of a local nh daemon write the clipboard ([48bb168](https://github.com/nikodem-wrona/neohar/commit/48bb168deac0131e28a561966b8bb658942bff95))
- **remote:** stop the reconnect to a socket of another user ([8949bec](https://github.com/nikodem-wrona/neohar/commit/8949becc6d13d632e78472a5fb94eb18db2c3e6b))
- **ui:** tell the person to answer a display-only card in the harness ([e65621f](https://github.com/nikodem-wrona/neohar/commit/e65621fc01933eb9d8e0e553fb144bbc7cd2e507))
- **rich:** open only regular files in the scan of a project ([ab9c9b2](https://github.com/nikodem-wrona/neohar/commit/ab9c9b26bce3d9db3a949388e73a312e12f6eeaf))
- **core:** run git with settings that a repository cannot change ([9e75297](https://github.com/nikodem-wrona/neohar/commit/9e752973c10ca29b8fddeea119bd06a2e643b09b))
- **rich:** score the options that run a program or write a file ([6d0b801](https://github.com/nikodem-wrona/neohar/commit/6d0b801475343a5abf924784ac6d5d349cb6ef1c))
- **core:** read a wrapper of a deny only in the place of the program ([6450fe8](https://github.com/nikodem-wrona/neohar/commit/6450fe86919687fa30d93f8a2b847519be1eb22d))
- **core:** let a session rule decide only up to the risk that was seen ([7227ff2](https://github.com/nikodem-wrona/neohar/commit/7227ff29a08915a4213a006fbc03d6ac407fd3e0))
- **remote:** keep the tray of link/1 readable across versions ([97c11d4](https://github.com/nikodem-wrona/neohar/commit/97c11d4894a77d66d831fc9df853226f06862f3a))
- **desktop:** focus a remote window that the person just asked for ([57b9bb1](https://github.com/nikodem-wrona/neohar/commit/57b9bb1eaaa774966ace7ec112cebc9aee352c33))
- **core:** refuse input from a pane to a pane that waits for an answer ([9d7452e](https://github.com/nikodem-wrona/neohar/commit/9d7452e95020a261e5bf5d136afc95c05dfa961e))
- **agents:** read the command of a dialog only from a sure frame ([f394cee](https://github.com/nikodem-wrona/neohar/commit/f394ceef2d80618df6a67b6692f4d74d91629cc6))
- close the gaps in the trust fingerprint of a repository ([c4318f0](https://github.com/nikodem-wrona/neohar/commit/c4318f09de1bd8e6ed30512918fb077b18e1ce3e))

## [1.0.0] - 2026-10-21

The first public release.

### Added

- A desktop app for macOS with workspaces, tabs and panes. A Rust core owns the terminals and
  draws them with WebGL2.
- Claude Code and Codex in panes. The board shows the state of each agent. The state comes from
  the hooks of the harness, and from the screen only when no hook is available.
- Approvals and questions as cards, as notifications or as CLI calls. Policy rules can answer them.
- Panes that continue to run when the app quits, crashes or updates. A small holder process owns
  each pane, and the next launch restores the screens.
- Tasks, each with its own git worktree.
- The Files tab and the Changes tab, and a search.
- The `nh` command line. It uses the same JSON-RPC methods as the app.
- Remote machines: the app connects over ssh to an `nh daemon` on another Mac and shows its panes.
- A phone app. The core serves it on `127.0.0.1`, and `tailscale serve` puts it on your tailnet. A
  paired phone shows the desk and answers requests. It gets a push when an agent needs you and no
  desktop window has the focus.

### Known limits

- macOS is the only tested platform.
- Builds have an ad-hoc signature. They are for local use only, and the app has no updater.

[Unreleased]: https://github.com/nikodem-wrona/neohar/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/nikodem-wrona/neohar/compare/v1.0.0...v2.0.0
[1.0.0]: https://github.com/nikodem-wrona/neohar/releases/tag/v1.0.0
