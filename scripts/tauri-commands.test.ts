import { describe, expect, it } from 'bun:test';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import {
  capabilityProblems,
  COMMANDS,
  fileProblems,
  listedCommands,
  readRepoFile,
  relativeTo,
  webviewFiles,
  WINDOW_PERMISSIONS,
} from './tauri-commands';

const root = join(import.meta.dir, '..');
const tauri = ['apps', 'desktop', 'src-tauri'];

describe('the webview calls three commands of the host and no other', () => {
  it('has no invoke, no @tauri-apps import and no plugin command outside the transport', () => {
    const files = webviewFiles(root);
    expect(files.length).toBeGreaterThan(10);
    const problems = files.flatMap((file) =>
      fileProblems(relativeTo(root, file), readRepoFile(root, relativeTo(root, file))),
    );
    expect(problems).toEqual([]);
  });

  it('has the transport call exactly rpc, subscribe and stream', () => {
    const text = readRepoFile(root, 'packages', 'transport', 'src', 'tauri.ts');
    const called = [...text.matchAll(/invoke\(\s*'([^']*)'/g)].map((m) => m[1]);
    expect([...new Set(called)].sort()).toEqual([...COMMANDS].sort());
  });

  it('has the host register the same three commands, and declare them in its manifest', () => {
    const lib = readRepoFile(root, ...tauri, 'src', 'lib.rs');
    const registered = listedCommands(lib, /generate_handler!\[([^\]]*)\]/);
    expect(registered).toEqual([...COMMANDS]);
    const build = readRepoFile(root, ...tauri, 'build.rs');
    expect(listedCommands(build, /\.commands\(&\[([^\]]*)\]\)/)).toEqual([...COMMANDS]);
  });

  it('has capabilities that allow those three, the drag and the zoom of the window, and nothing else', () => {
    const dir = join(root, ...tauri, 'capabilities');
    const files = readdirSync(dir)
      .filter((name) => name.endsWith('.json'))
      .map((name) => ({ name, json: readRepoFile(root, ...tauri, 'capabilities', name) }));
    expect(files.length).toBeGreaterThan(0);
    expect(capabilityProblems(files)).toEqual([]);
  });

  it('does not put the Tauri API on window', () => {
    const conf = JSON.parse(readRepoFile(root, ...tauri, 'tauri.conf.json')) as {
      app: { withGlobalTauri?: boolean };
    };
    expect(conf.app.withGlobalTauri).toBe(false);
  });
});

describe('the checks catch a violation', () => {
  it('rejects an invoke outside the transport', () => {
    const problems = fileProblems('apps/desktop/src/x.ts', "invoke('rpc', {})");
    expect(problems).toHaveLength(1);
  });

  it('rejects a command that is not one of the three', () => {
    const file = 'packages/transport/src/x.ts';
    expect(fileProblems(file, "await invoke('read_file', {})")).toEqual([
      `${file}: invoke of read_file`,
    ]);
    expect(fileProblems(file, "await invoke<string>('rpc', {})")).toEqual([]);
  });

  it('rejects a command name that is not a literal', () => {
    const file = 'packages/transport/src/x.ts';
    expect(fileProblems(file, 'invoke(name, {})')).toHaveLength(1);
  });

  it('rejects an import of the Tauri API in the UI, and a plugin command anywhere', () => {
    expect(
      fileProblems('packages/ui/src/x.ts', "import { x } from '@tauri-apps/api/core';"),
    ).toHaveLength(1);
    expect(fileProblems('packages/transport/src/x.ts', "const c = 'plugin:fs|read';")).toHaveLength(
      1,
    );
  });

  it('rejects a capability with an extra permission, a scope, or a remote origin', () => {
    const grant = (permissions: unknown[], extra = {}) => [
      { name: 'c.json', json: JSON.stringify({ permissions, ...extra }) },
    ];
    // The three commands, and the drag and the zoom of the window from the command line.
    const all = ['allow-rpc', 'allow-subscribe', 'allow-stream', ...WINDOW_PERMISSIONS];
    expect(capabilityProblems(grant(all))).toEqual([]);
    expect(capabilityProblems(grant([...all, 'core:default']))).toHaveLength(1);
    expect(capabilityProblems(grant([...all, 'core:window:allow-close']))).toHaveLength(1);
    expect(capabilityProblems(grant(all.filter((p) => p !== 'allow-subscribe')))).toEqual([
      'capabilities lack allow-subscribe',
    ]);
    expect(capabilityProblems(grant([...all, { identifier: 'fs:allow-read' }]))).toHaveLength(1);
    expect(capabilityProblems(grant(all, { remote: { urls: ['https://x'] } }))).toHaveLength(1);
  });
});
