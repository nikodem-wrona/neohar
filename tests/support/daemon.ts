// A headless core for the Playwright specs. It builds `nh` with the feature `dev-ws`, starts the
// foreground form `nh daemon --dev-ws` (a test seam) with temporary directories, and says where
// the core listens. The specs run only locally, because they need a browser.
//
// The core and its panes get a temporary `HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME`, so that
// they never read or write the real `~/.claude`, `~/.codex` or the worktrees under `~/.neohar`.
// `PATH` stays, so that a pane finds the programs of the machine.
//
// With `throughLink`, the daemon serves the dev WebSocket from a client of its own `link.sock`
// (`--dev-ws-through-link`), so that every call, projection and frame passes `link/1`.
//
// With `phonePort`, the daemon also serves the listener of the phone on that port of `127.0.0.1`
// (`--phone-port`), and `webDist` names the build of the phone app that `nh` takes into
// its binary (`NEOHAR_WEB_DIST`). `halt` stops the core and keeps its directories, and `resume`
// starts it again on them and on the same port of the phone, as a Mac that comes back.

import { mkdir, mkdtemp, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export const root = join(import.meta.dir, '..', '..');

export interface DaemonInfo {
  ws_port: number;
  token: string;
  api_socket: string;
  link_socket: string;
  /** The port of the listener of the phone, with `phonePort`. */
  phone_port?: number;
}

export interface Daemon {
  info: DaemonInfo;
  /** The path of the `nh` binary that the daemon runs. */
  nh: string;
  /** The directory of `config.toml`. A spec writes the file there and reloads the configuration. */
  configDir: string;
  /**
   * The folder of the fake `tailscale` of the core (`tests/support/fake-tailscale.sh`). A spec
   * writes `status.json` there. The core never runs the Tailscale of the machine.
   */
  tailscaleDir: string;
  /** Stops the core and its programs, and keeps the directories. */
  halt(): Promise<void>;
  /**
   * Kills the core with no time to stop its programs, as a crash does. The holders and their
   * programs keep running, and `resume` restores the panes from them.
   */
  crash(): Promise<void>;
  /** Starts a halted core again with the same directories. Gives the new info. */
  resume(): Promise<DaemonInfo>;
  /** Stops the programs of the core and removes the temporary directories. */
  stop(): Promise<void>;
}

export interface DaemonOptions {
  throughLink?: boolean;
  /** The port of the listener of the phone. 0 is a free port. */
  phonePort?: number;
  /** The folder of the built phone app, for the build of `nh`. */
  webDist?: string;
}

export async function run(cmd: string[], env?: Record<string, string>): Promise<void> {
  const p = Bun.spawn(cmd, {
    cwd: root,
    stdout: 'inherit',
    stderr: 'inherit',
    ...(env ? { env: { ...process.env, ...env } } : {}),
  });
  if ((await p.exited) !== 0) throw new Error(`${cmd.join(' ')} failed`);
}

export async function startDaemon(
  prefix: string,
  options: boolean | DaemonOptions = false,
): Promise<Daemon> {
  const opts: DaemonOptions = typeof options === 'boolean' ? { throughLink: options } : options;
  await run(
    ['cargo', 'build', '--locked', '-p', 'nh', '--features', 'dev-ws'],
    opts.webDist ? { NEOHAR_WEB_DIST: opts.webDist } : undefined,
  );
  const dir = await mkdtemp(join(tmpdir(), `${prefix}-`));
  // The runtime directory holds Unix sockets, which have a short limit on the length of a path.
  const runtime = await mkdtemp('/tmp/nht-');
  const nh = join(root, 'target', 'debug', 'nh');
  const tailscaleDir = join(dir, 'tailscale');
  await mkdir(tailscaleDir);
  const home = join(dir, 'home');
  await mkdir(join(home, '.claude'), { recursive: true });
  await mkdir(join(home, '.codex'));
  // A link, not a copy: macOS checks each new program before its first start. The script finds
  // its folder from the link.
  await symlink(
    join(root, 'tests', 'support', 'fake-tailscale.sh'),
    join(tailscaleDir, 'tailscale'),
  );
  const args = [nh, 'daemon', opts.throughLink ? '--dev-ws-through-link' : '--dev-ws'];
  if (opts.phonePort !== undefined) args.push('--phone-port', String(opts.phonePort));

  const spawn = async () => {
    const daemon = Bun.spawn(args, {
      cwd: root,
      env: {
        ...process.env,
        HOME: home,
        CLAUDE_CONFIG_DIR: join(home, '.claude'),
        CODEX_HOME: join(home, '.codex'),
        NEOHAR_CONFIG_DIR: join(dir, 'config'),
        NEOHAR_DATA_DIR: join(dir, 'data'),
        NEOHAR_RUNTIME_DIR: runtime,
        NEOHAR_NH: nh,
        // A debug build of `nh` takes this CLI of Tailscale.
        NEOHAR_TAILSCALE: join(tailscaleDir, 'tailscale'),
        SHELL: '/bin/sh',
      },
      stdin: 'pipe',
      stdout: 'pipe',
      stderr: 'inherit',
      // Its own process group: a signal to the group of the test server (Playwright sends one when
      // it ends the server) must not kill the core before it stops its programs. The holders outlive
      // a core that dies, by design.
      detached: true,
    });
    const reader = daemon.stdout.getReader();
    const first = await reader.read();
    const info = JSON.parse(
      new TextDecoder().decode(first.value).split('\n')[0] ?? '{}',
    ) as DaemonInfo;
    if (!info.ws_port) throw new Error('the daemon did not say where it listens');
    return { daemon, info };
  };
  /** Ends a core: it stops every program when its input closes. */
  const end = async (daemon: Awaited<ReturnType<typeof spawn>>['daemon']) => {
    daemon.stdin.end();
    await Promise.race([daemon.exited, Bun.sleep(15_000)]);
    daemon.kill();
    await daemon.exited;
  };

  let current: Awaited<ReturnType<typeof spawn>> | null = await spawn();
  // A core that starts again keeps the port of the phone, so the page of a test finds it.
  if (current.info.phone_port !== undefined && opts.phonePort === 0) {
    args.splice(args.indexOf('--phone-port') + 1, 1, String(current.info.phone_port));
  }
  const daemon: Daemon = {
    info: current.info,
    nh,
    configDir: join(dir, 'config'),
    tailscaleDir,
    async halt() {
      if (current) await end(current.daemon);
      current = null;
    },
    async crash() {
      if (current) {
        current.daemon.kill('SIGKILL');
        await current.daemon.exited;
      }
      current = null;
    },
    async resume() {
      if (current) return current.info;
      current = await spawn();
      daemon.info = current.info;
      return current.info;
    },
    async stop() {
      if (current) await end(current.daemon);
      current = null;
      await rm(dir, { recursive: true, force: true });
      await rm(runtime, { recursive: true, force: true });
    },
  };
  return daemon;
}
