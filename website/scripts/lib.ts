// Helpers shared by the generators of the documentation site.
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

/** The root of the repository. */
export const repoRoot = resolve(import.meta.dir, '..', '..');

/** The `docs/` directory of the site. */
export const docsRoot = resolve(import.meta.dir, '..', 'docs');

export const githubBlob = 'https://github.com/nikodem-wrona/neohar/blob/main';

/** The first line of every generated page, so a person knows not to edit it. */
export function generatedNote(source: string, generator: string): string {
  return [
    ':::info[Generated page]',
    '',
    `This page is generated from [\`${source}\`](${githubBlob}/${source}) by \`website/scripts/${generator}\`.`,
    'Do not edit it by hand. Change the source and run `bun run gen` in `website/`.',
    '',
    ':::',
    '',
  ].join('\n');
}

/**
 * `--check`: the generators write nothing. They compare each file with the file that is there, and
 * `checkFaults` gives the files that are different (`mise run check:docs`).
 */
export const checkOnly = process.argv.includes('--check');

const different: string[] = [];
const written = new Set<string>();
const cleared = new Set<string>();

/**
 * The text of a file for a comparison. The note of a CLI page names the binary that made it, and
 * a debug and a release build give the same help.
 */
const comparable = (text: string) =>
  text.replace(/`target\/(debug|release)\/nh`/g, '`target/…/nh`');

/** Writes a file and makes its directory. With `--check`, it compares the file and writes nothing. */
export function write(path: string, text: string): void {
  const body = text.endsWith('\n') ? text : `${text}\n`;
  if (checkOnly) {
    written.add(path);
    let old: string | undefined;
    try {
      old = readFileSync(path, 'utf8');
    } catch {
      old = undefined;
    }
    if (old === undefined || comparable(old) !== comparable(body)) {
      different.push(relative(repoRoot, path));
    }
    return;
  }
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, body);
}

/** Removes the generated `.md` files of a directory, and keeps `_category_.json` and hand pages. */
export function clearGenerated(dir: string, keep: (name: string) => boolean = () => false): void {
  let names: string[];
  try {
    names = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of names) {
    if (!name.endsWith('.md') || keep(name)) continue;
    if (checkOnly) cleared.add(join(dir, name));
    else rmSync(join(dir, name));
  }
}

/** After a run with `--check`: the files that a run without it would change, add or remove. */
export function checkFaults(): string[] {
  const removed = [...cleared]
    .filter((path) => !written.has(path))
    .map((path) => `${relative(repoRoot, path)} (no generator writes it now)`);
  return [...different, ...removed];
}

/** Front matter for a page. Values are quoted as JSON strings, which YAML accepts. */
export function frontMatter(fields: Record<string, string | number | boolean>): string {
  const lines = Object.entries(fields).map(([key, value]) =>
    typeof value === 'string' ? `${key}: ${JSON.stringify(value)}` : `${key}: ${value}`,
  );
  return ['---', ...lines, '---', ''].join('\n');
}

/** The anchor of a name: lowercase, every character that is not a letter or digit is a hyphen. */
export function anchor(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

/** Makes text safe for one cell of a Markdown table. */
export function cell(text: string): string {
  return text
    .replace(/\r?\n+/g, ' ')
    .replace(/\|/g, '\\|')
    .trim();
}
