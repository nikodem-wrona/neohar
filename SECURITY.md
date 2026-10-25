# Security policy

## Supported versions

| Version | Supported |
|---|---|
| The latest release | Yes |
| Earlier releases | No |

A security fix goes into a new release. We do not make fixes for earlier releases.

## Report a vulnerability

Do not report a vulnerability in a public issue, a pull request or a discussion.

Use the private vulnerability reporting of GitHub:

1. Open the **Security** tab of the repository.
2. Click **Report a vulnerability**.
3. Give the version of neohar (`nh --version`), the version of macOS, the steps to reproduce the problem, and its effect.

The maintainer replies in the private report. When a fix is available, we publish a release and a security advisory.

## Scope

These parts of neohar are in scope:

- **The core** (`nh-core`) and its sockets: the API socket that the CLI and the hooks use, and `link.sock`.
- **The holder** (`nh hold`): the process that owns the PTY of each pane, its journal and its socket protocol.
- **The hooks**: the hook shim (`nh hook`), and the hooks that `nh integration install` writes into the configuration of Claude Code and Codex.
- **The phone listener** (`nh-web`). This includes the pairing, the cookie of a paired phone and the checks of `Host` and `Origin`. It also includes the scope of the calls of a phone and the list of push services.
- **The link to remote machines over ssh**: `nh daemon`, `nh bridge` and `nh askpass`. The app does not trust the core of a remote machine. That core must stay limited to its own panes: it must not open, read or start anything on your Mac, it must not write your clipboard or take your keyboard without an action of yours, and its HTML must not escape the sandbox of the webview. A way around one of these limits is in scope.
- **The webview sandbox**: the three commands of the webview, the content security policy, and the `nh://` URLs.
- **Deep links** (`neohar://`): any web page can open one, so a link must not answer a request or give a pane the focus.
- **Other users of the same machine**: an attack from another local user, for example a socket, a directory or a port of `127.0.0.1` that this user makes before neohar does.
- **The git commands of the Changes tab and the Git view**: the trust of a repository, and the settings that neohar gives to each git command.

These items are not in scope:

- Claude Code, Codex and their permission systems. Report a problem in a harness to Anthropic or to OpenAI.
- Tailscale, OpenSSH and macOS.
- An attack that needs code that already runs as your macOS user.
- The ad-hoc signature of the builds. It is a known limit: there is no notarized build.
- The timing of the keys that you type in a window of a remote machine. A person who watches the network can see it, because the link sends each key at once and ssh does not hide the timing of a session without a terminal. [Remote machines](website/docs/guides/remote-machines.md) tells more.

## Limits of the approval gate

The approval gate of neohar is not a sandbox. It shows the requests of an agent and carries your answers. It does not contain the agent.

- A process that runs as your user and can connect to `api.sock` can drive neohar. It can answer approvals, change the policy and type into panes. The environment of each pane names the socket (`NEOHAR_SOCKET`).
- neohar refuses the plain path from inside a pane. When a client of the API socket calls a method that answers or releases an interaction, changes a policy rule, pairs a phone, trusts a repository, or sets a key of `approvals`, `policy`, `phone` or `machines`, the core reads the parent processes of the client. If the holder of a pane is one of them, or the client is in the session of the program of a pane, the core refuses the call.
- A key that goes into the dialog of a harness answers it. So the core also refuses input from inside a pane (`pane.paste`, `pane.send_bytes`, `agent.prompt`, `agent.reply`, `agent.interrupt`) to a pane that waits for an answer: its agent has an open approval, question or plan. This includes the pane of the client. Input to a pane that waits for no answer is still possible from inside a pane, for example to a shell or to an idle agent.
- A process that leaves the process tree of its pane can bypass this check. An example is a process that starts a session of its own and whose parent then ends.
- Configure the sandbox of each harness so that it cannot reach Unix sockets in the runtime directory of neohar: `/tmp/neohar-$UID/` on macOS, `$XDG_RUNTIME_DIR` on Linux. The sandbox of Codex and the sandboxed Bash of Claude Code can permit or refuse this access.

## Data on disk

neohar keeps its data in `state.db` and in the scrollback archive, in the data directory of the platform. The files have mode 0600.

- The interactions keep their payload: the command, the input of the tool, a diff with the contents of files, and the last lines of the pane. When an interaction ends, neohar removes the secrets that it can find from the stored payload and decision (`nh-redact`). The redaction can miss a secret. An open interaction keeps its full payload, because the core needs it to deliver the answer after a restart.
- neohar deletes the ended interactions and their delivery records when they are older than `events.audit_retention` (90 days by default).
- The scrollback of each pane, and its search index, stay on disk with no limit by default. Set `scrollback.max_bytes_per_pane` in `config.toml` to limit the archive of each pane. The text of the scrollback is not redacted.

## Git repositories

The settings of a git repository can name programs that git runs. Thus neohar runs no git command in a repository until you trust it: the Changes tab and the Git view show a **Trust** button. Trust a repository only when you know where it comes from. When a setting that names a program changes after the trust, neohar asks again. [Trust a repository](website/docs/guides/files-and-changes.md#trust-a-repository) gives the details.
