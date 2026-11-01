---
title: Hermes Agent
description: "Chat with Hermes Agent of Nous Research in the Hermes view: set up Hermes, start a chat in a folder, answer approvals, and what happens when the app quits."
sidebar_position: 5.5
---

[Hermes Agent](https://hermes-agent.nousresearch.com/docs) is an agent of Nous Research. neohar
shows it in a chat, not in a terminal pane. The core runs `hermes acp` and speaks the Agent Client
Protocol (ACP) to it. The gateway of Hermes owns each session, so a turn continues when the app
quits. When the app starts again, the core resumes the sessions and the chats come back.

## Set up Hermes

Do these steps one time, in a terminal:

1. Install Hermes Agent with the installer of its docs.
2. Set a model provider with `hermes model`. For a ChatGPT or Codex subscription, use
   `hermes auth add openai-codex`.
3. Run `hermes acp --check`. It must say `Hermes ACP check OK`.

`hermes.command` and `hermes.args` in the config of neohar give the program. The defaults are
`hermes` and `acp`. The core finds the program with the `PATH` of your login shell.

## Chat

Press `⌘⇧H`, or click **Hermes** in the status bar, to open the Hermes view. Do the same to close
it. The view takes the middle of the window, as the Workflows view does, and the panes continue to
run. The phone app has no Hermes view.

- **New chat** starts a chat in the folder of the window. Hermes works in that folder.
- The left column lists the open chats with their state: **Starting**, **Idle**, **Working**,
  **Approval** or **Disconnected**.
- Enter sends the prompt. Shift+Enter makes a new line. A prompt during a turn waits for the end of
  that turn.
- **Stop**, or Escape in an empty field, stops the turn.
- **Model** and **Mode** change the model and the edit mode of the chat. **Default** asks before
  each edit; **Accept Edits** and **Don't Ask** allow more.
- **Close** removes the chat from neohar. The session stays in Hermes.

## Approvals

An approval of Hermes shows as a card in the chat, with the options that Hermes gives. A file edit
asks in the mode **Default**. A shell command asks only when `approvals.mode` of Hermes is
`manual`. With `smart` (the default of Hermes), a model of Hermes decides, and the view says so.

Hermes denies an approval that nobody answers after `approvals.timeout` of Hermes, also when neohar
is closed. The default is 300 seconds. Set `hermes.approval_timeout_s` of neohar to the same value,
so that the card shows the right state.

## Limits

- A turn that Hermes continued while the app was closed shows **Working?**: ACP tells the end of a
  turn only to the client that sent the prompt, so neohar guesses it.
- neohar shows only the chats that it started. A session of the Hermes TUI is not in the list.
- Hermes has no question tool in ACP, so it cannot ask a question in the chat.
