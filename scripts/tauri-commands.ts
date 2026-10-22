// The rule: the webview calls exactly three commands of the host, `rpc`, `subscribe` and
// `stream`. This module finds what breaks the rule. `tauri-commands.test.ts` runs it over the
// repository.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

export const COMMANDS = ['rpc', 'subscribe', 'stream'] as const;

/** The two calls of Tauri that act on the window of the page: the command line is the title bar
 *  on macOS, so a press on it moves the window and a double click zooms it. The script of
 *  Tauri for `data-tauri-drag-region` makes these calls, not the code of the webview. */
export const WINDOW_PERMISSIONS = [
  'core:window:allow-start-dragging',
  'core:window:allow-internal-toggle-maximize',
] as const;

/** The only directory that may import `@tauri-apps/api` and call `invoke`. */
export const TRANSPORT_DIR = join('packages', 'transport', 'src');

const SOURCE = /\.(ts|tsx|js|jsx)$/;

/** The files of the webview code: the app entry and the `src` of every package. */
export function webviewFiles(root: string): string[] {
  const dirs = [join(root, 'apps', 'desktop', 'src')];
  const packages = join(root, 'packages');
  for (const name of readdirSync(packages)) {
    const src = join(packages, name, 'src');
    if (exists(src)) dirs.push(src);
  }
  return dirs.flatMap((dir) => walk(dir));
}

function exists(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function walk(dir: string): string[] {
  if (!exists(dir)) return [];
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (name === 'node_modules' || name === 'dist') return [];
    return statSync(path).isDirectory() ? walk(path) : SOURCE.test(name) ? [path] : [];
  });
}

/** What is wrong in one source file. `file` is relative to the repository root. */
export function fileProblems(file: string, text: string): string[] {
  const problems: string[] = [];
  const inTransport = file.startsWith(TRANSPORT_DIR + sep);
  const imports = /from\s+['"]@tauri-apps\/[^'"]*['"]|import\s*\(\s*['"]@tauri-apps\//.test(text);
  if (imports && !inTransport) {
    problems.push(`${file}: imports @tauri-apps outside ${TRANSPORT_DIR}`);
  }
  if (/\bwindow\.__TAURI/.test(text) && !inTransport) {
    problems.push(`${file}: reaches into window.__TAURI*`);
  }
  for (const call of text.matchAll(/\binvoke\s*(?:<[^>(]*>)?\s*\(\s*([^),]*)/g)) {
    if (!inTransport) {
      problems.push(`${file}: calls invoke outside ${TRANSPORT_DIR}`);
      continue;
    }
    const literal = /^(['"])([^'"]*)\1$/.exec((call[1] ?? '').trim());
    if (!literal) {
      problems.push(`${file}: invoke without a literal command name: ${call[0]}`);
    } else if (!(COMMANDS as readonly string[]).includes(literal[2] ?? '')) {
      problems.push(`${file}: invoke of ${literal[2]}`);
    }
  }
  if (/['"`]plugin:/.test(text)) problems.push(`${file}: names a plugin command`);
  return problems;
}

/** The permissions that the capability files grant, and what is wrong with them. */
export function capabilityProblems(files: { name: string; json: string }[]): string[] {
  const problems: string[] = [];
  const granted = new Set<string>();
  for (const { name, json } of files) {
    const capability = JSON.parse(json) as { permissions?: unknown[]; remote?: unknown };
    if (capability.remote) problems.push(`${name}: grants a remote origin`);
    for (const permission of capability.permissions ?? []) {
      if (typeof permission !== 'string') {
        problems.push(`${name}: a permission with a scope: ${JSON.stringify(permission)}`);
      } else {
        granted.add(permission);
      }
    }
  }
  const wanted = [...COMMANDS.map((c) => `allow-${c}`), ...WINDOW_PERMISSIONS];
  for (const permission of granted) {
    if (!wanted.includes(permission)) problems.push(`capabilities grant ${permission}`);
  }
  for (const permission of wanted) {
    if (!granted.has(permission)) problems.push(`capabilities lack ${permission}`);
  }
  return problems;
}

/** The commands that a Rust source lists, in `generate_handler![…]` or `.commands(&[…])`. */
export function listedCommands(rust: string, pattern: RegExp): string[] {
  const match = pattern.exec(rust);
  return (match?.[1] ?? '')
    .split(',')
    .map((item) => item.trim().replace(/^"|"$/g, ''))
    .filter((item) => item !== '');
}

export function readRepoFile(root: string, ...parts: string[]): string {
  return readFileSync(join(root, ...parts), 'utf8');
}

export function relativeTo(root: string, file: string): string {
  return relative(root, file);
}
