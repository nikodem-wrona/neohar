// `mise run check:bundle`: the bundles of the webview stay small.
// Builds nothing. It reads apps/desktop/dist and target/web-dist after their builds and adds the
// gzipped size of every script, style sheet and WASM file of each. Each total must stay under the
// limit of its bundle. The phone app also has a limit for its first chunk: the files that its
// page loads before the first paint (the script, the style sheet and the module preloads of
// `index.html`). The renderer and the decoder load after it. The run prints the breakdown, so that
// a commit can show where the bytes went.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { extname, join, relative } from 'node:path';
import { gzipSync } from 'node:zlib';

/** The budget of the desk for scripts, styles and WASM together, in gzipped bytes: 300 KB. */
export const LIMIT = 300_000;

/** The budget of the first chunk of the phone app, before the surface and the WASM: 150 KB. */
export const PHONE_LIMIT = 150_000;

/**
 * The bundles that the check reads: the folder under the root of the repository, the limit of the
 * whole bundle, and the limit of the first chunk, if it has one.
 */
export const BUNDLES = [
  { name: 'desk', dir: join('apps', 'desktop', 'dist'), limit: LIMIT, first: null },
  { name: 'phone', dir: join('target', 'web-dist'), limit: LIMIT, first: PHONE_LIMIT },
] as const;

/**
 * The files of the first chunk: the scripts, the style sheets and the module preloads that the
 * page names under `/assets/`. `/theme.css` of the listener is not a file of the build.
 */
export function firstFiles(page: string): string[] {
  const found = new Set<string>();
  for (const match of page.matchAll(/(?:src|href)="\/(assets\/[^"]+\.(?:js|css))"/g)) {
    if (match[1]) found.add(match[1]);
  }
  return [...found].sort();
}

/** The kinds of file that count. Source maps and the HTML page do not ship in the page's weight. */
const COUNTED = new Set(['.js', '.css', '.wasm']);

export interface Part {
  file: string;
  raw: number;
  gzip: number;
}

/** The files under `dir` that count, with their sizes, the largest first. */
export function measure(dir: string): Part[] {
  const parts: Part[] = [];
  const walk = (current: string): void => {
    for (const name of readdirSync(current)) {
      const path = join(current, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (COUNTED.has(extname(name))) {
        const bytes = readFileSync(path);
        parts.push({
          file: relative(dir, path),
          raw: bytes.length,
          gzip: gzipSync(bytes, { level: 9 }).length,
        });
      }
    }
  };
  walk(dir);
  return parts.sort((a, b) => b.gzip - a.gzip || a.file.localeCompare(b.file));
}

export function total(parts: readonly Part[]): number {
  return parts.reduce((sum, part) => sum + part.gzip, 0);
}

/** The breakdown as text lines. */
export function table(parts: readonly Part[], limit: number = LIMIT): string {
  const rows = parts.map(
    (part) =>
      `${String(part.gzip).padStart(8)} gz ${String(part.raw).padStart(8)} raw  ${part.file}`,
  );
  const sum = total(parts);
  rows.push(`${String(sum).padStart(8)} gz ${String(limit).padStart(8)} limit`);
  return rows.join('\n');
}

if (import.meta.main) {
  let failed = 0;
  for (const bundle of BUNDLES) {
    const dir = join(import.meta.dir, '..', bundle.dir);
    if (!existsSync(dir)) {
      console.error(`check:bundle: ${dir} is missing. Run the build of the ${bundle.name} first.`);
      process.exit(2);
    }
    const parts = measure(dir);
    console.log(`${bundle.name} (${bundle.dir})\n${table(parts, bundle.limit)}`);
    if (parts.length === 0) {
      console.error(`check:bundle: the ${bundle.name} has no script, style sheet or WASM file.`);
      process.exit(2);
    }
    if (total(parts) > bundle.limit) {
      console.error(
        `check:bundle: the ${bundle.name} has ${total(parts)} bytes gzipped, over its limit of ${bundle.limit}.`,
      );
      failed = 1;
    }
    if (bundle.first !== null) {
      const page = readFileSync(join(dir, 'index.html'), 'utf8');
      const names = firstFiles(page);
      const first = parts.filter((part) => names.includes(part.file));
      console.log(`${bundle.name}, first chunk\n${table(first, bundle.first)}`);
      if (first.length === 0) {
        console.error(`check:bundle: the page of the ${bundle.name} names no script.`);
        process.exit(2);
      }
      if (total(first) > bundle.first) {
        console.error(
          `check:bundle: the first chunk of the ${bundle.name} has ${total(first)} bytes gzipped, over its limit of ${bundle.first}.`,
        );
        failed = 1;
      }
    }
  }
  process.exit(failed);
}
