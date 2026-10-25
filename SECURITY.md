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
- **The link to remote machines over ssh**: `nh daemon`, `nh bridge` and `nh askpass`.
- **The webview sandbox**: the three commands of the webview, the content security policy, and the `nh://` URLs.

These items are not in scope:

- Claude Code, Codex and their permission systems. Report a problem in a harness to Anthropic or to OpenAI.
- Tailscale, OpenSSH and macOS.
- An attack that needs code that already runs as your macOS user.
- The ad-hoc signature of the builds. It is a known limit: there is no notarized build.

## Limits of the approval gate

The approval gate of neohar is not a sandbox. It shows the requests of an agent and carries your answers. It does not contain the agent.

- A process that runs as your user and can connect to `api.sock` can drive neohar. It can answer approvals, change the policy and type into panes. The environment of each pane names the socket (`NEOHAR_SOCKET`).
- neohar refuses the plain path from inside a pane. When a client of the API socket calls a method that answers or releases an interaction, changes a policy rule, pairs a phone, or sets a key of `approvals`, `policy`, `phone` or `machines`, the core reads the parent processes of the client. If the holder of a pane is one of them, or the client is in the session of the program of a pane, the core refuses the call.
- A process that leaves the process tree of its pane can bypass this check. An example is a process that starts a session of its own and whose parent then ends.
- Configure the sandbox of each harness so that it cannot reach Unix sockets in the runtime directory of neohar: `/tmp/neohar-$UID/` on macOS, `$XDG_RUNTIME_DIR` on Linux. The sandbox of Codex and the sandboxed Bash of Claude Code can permit or refuse this access.
