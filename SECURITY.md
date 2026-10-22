# Security policy

## Supported versions

| Version | Supported |
|---|---|
| 1.0.x | Yes |
| Earlier versions | No |

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
