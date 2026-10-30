// Writes THIRD_PARTY_LICENSES.md: the licenses of the code and the data of other authors that
// neohar.app contains. The Tauri bundle puts the file in Contents/Resources.
//
//   bun scripts/gen-notices.ts           `mise run gen:notices`
//   bun scripts/gen-notices.ts --check   `mise run check:notices`: fails when the file is out of date
//
// Rust: cargo-about (about.toml) reads the dependencies of each crate that ships, for the targets
// of the app. It runs offline and reads only the files of the crates in the cargo registry. The
// script merges the results, so that each license text comes one time.
//
// JavaScript: the packages in `dependencies` of the two apps, through their workspace packages, and
// the dependencies of those packages.
// The devDependencies do not ship. Two more items are in the bundles: the tslib helpers that
// @tauri-apps/api copies into its code, and the module preload code of Vite. When a dependency
// changes, look at the `sources` of the source map of apps/desktop to find what the bundle has.
//
// Data: the art of material-icon-theme (`mise run gen:icons`) and the syntax definitions that
// syntect bundles.

import { existsSync, readdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const target = join(root, 'THIRD_PARTY_LICENSES.md');

/** The crates that ship, with the arguments of cargo-about for each. */
export const RUST_ROOTS = [
  // The app. `cargo tauri build` turns on `custom-protocol`.
  { manifest: 'apps/desktop/src-tauri/Cargo.toml', args: ['--features', 'custom-protocol'] },
  // The sidecar: holders, the hook shim, the CLI and `nh daemon`.
  { manifest: 'crates/nh/Cargo.toml', args: [] },
  // The frame decoder in the webview and in the phone app.
  { manifest: 'packages/proto-wasm/Cargo.toml', args: ['--target', 'wasm32-unknown-unknown'] },
];

/** The packages whose `dependencies` go into a bundle. */
const JS_APPS = ['apps/desktop', 'apps/web'];

/** The license of the syntax definitions in syntect (`default-syntaxes`), from sublimehq/Packages. */
const SUBLIME_PACKAGES_LICENSE = `If not otherwise specified (see below), files in this repository fall under the following license:

    Permission to copy, use, modify, sell and distribute this
    software is granted. This software is provided "as is" without
    express or implied warranty, and with no claim as to its
    suitability for any purpose.

An exception is made for files in readable text which contain their own license information, or files where an accompanying file exists (in the same directory) with a “-license” suffix added to the base-name name of the original file, and an extension of txt, html, or similar. For example “tidy” is accompanied by “tidy-license.txt”.`;

/** One license text and the crates or packages that use it. */
export interface License {
  id: string;
  name: string;
  text: string;
  users: string[];
}

/** The part of the JSON output of cargo-about that this script reads. */
export interface AboutOutput {
  licenses: {
    id: string;
    name: string;
    text: string;
    used_by: { crate: { name: string; version: string } }[];
  }[];
}

/** One item of the JavaScript or the data sections. */
export interface Item {
  title: string;
  license: string;
  what: string;
  texts: string[];
}

/** Line ends as `\n`, no trailing spaces, no empty lines at the start or the end. */
export function clean(text: string): string {
  return text
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .replace(/^\n+|\n+$/g, '');
}

/** Merges the outputs of cargo-about: one entry for each license text, sorted. */
export function mergeRust(outputs: AboutOutput[]): License[] {
  const byText = new Map<string, { id: string; name: string; text: string; users: Set<string> }>();
  for (const output of outputs) {
    for (const license of output.licenses) {
      const text = clean(license.text);
      const key = `${license.id}\0${text}`;
      const entry = byText.get(key) ?? {
        id: license.id,
        name: license.name,
        text,
        users: new Set<string>(),
      };
      for (const user of license.used_by)
        entry.users.add(`${user.crate.name} ${user.crate.version}`);
      byText.set(key, entry);
    }
  }
  const sorted = [...byText.values()].map((entry) => ({
    id: entry.id,
    name: entry.name,
    text: entry.text,
    users: [...entry.users].sort(compare),
  }));
  return sorted.sort(
    (a, b) =>
      compare(a.id, b.id) || compare(a.users[0] ?? '', b.users[0] ?? '') || compare(a.text, b.text),
  );
}

function compare(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** A code fence that is longer than each run of backticks in `text`. */
function fence(text: string): string {
  const longest = Math.max(0, ...[...text.matchAll(/`+/g)].map((m) => m[0].length));
  return '`'.repeat(Math.max(3, longest + 1));
}

function block(text: string): string[] {
  const f = fence(text);
  return [`${f}text`, text, f];
}

/** The text of THIRD_PARTY_LICENSES.md. */
export function render(rust: License[], js: Item[], data: Item[]): string {
  const crates = new Set(rust.flatMap((license) => license.users));
  const counts = new Map<string, Set<string>>();
  const count = (id: string, user: string) => {
    const users = counts.get(id) ?? new Set<string>();
    users.add(user);
    counts.set(id, users);
  };
  for (const license of rust) for (const user of license.users) count(license.id, user);
  for (const item of [...js, ...data]) count(item.license, item.title);

  const out = [
    '# Third-party licenses',
    '',
    'neohar is under the Apache License 2.0 (the file LICENSE). neohar.app also contains code and',
    'data of other authors. This file gives their licenses.',
    '',
    'Do not edit this file. `mise run gen:notices` writes it from the lockfiles, the cargo registry',
    'and `node_modules`.',
    '',
    `- **Rust crates**: ${crates.size} crates in \`neohar-desktop\`, \`nh\` and the WASM decoder`,
    '  `proto-wasm`, for macOS and Linux.',
    `- **JavaScript**: ${js.length} items in the bundles of the webview and of the phone app.`,
    `- **Data**: ${data.length} items.`,
    '',
    '## Summary',
    '',
    '| License | Crates and packages |',
    '|---|---|',
    ...[...counts.entries()]
      .sort((a, b) => b[1].size - a[1].size || compare(a[0], b[0]))
      .map(([id, users]) => `| ${id} | ${users.size} |`),
    '',
    '## Rust crates',
    '',
  ];
  for (const license of rust) {
    out.push(`### ${license.name} (${license.id})`, '');
    out.push(`Used by: ${license.users.map((user) => `\`${user}\``).join(', ')}.`, '');
    out.push(...block(license.text), '');
  }
  for (const [heading, items] of [
    ['JavaScript', js],
    ['Data', data],
  ] as const) {
    out.push(`## ${heading}`, '');
    for (const item of items) {
      out.push(`### ${item.title} (${item.license})`, '', item.what, '');
      for (const text of item.texts) out.push(...block(text), '');
    }
  }
  return `${out.join('\n').trimEnd()}\n`;
}

/** Runs cargo-about for each root of RUST_ROOTS at the same time. */
async function aboutAll(): Promise<AboutOutput[]> {
  return Promise.all(
    RUST_ROOTS.map(async ({ manifest, args }) => {
      const proc = Bun.spawn(
        [
          'cargo',
          'about',
          'generate',
          '--frozen',
          '--fail',
          '--format',
          'json',
          '--config',
          join(root, 'about.toml'),
          '--manifest-path',
          join(root, manifest),
          ...args,
        ],
        { cwd: root, stdout: 'pipe', stderr: 'pipe' },
      );
      const [stdout, stderr, code] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
      ]);
      if (code !== 0) throw new Error(`cargo about failed for ${manifest}:\n${stderr}`);
      return JSON.parse(stdout) as AboutOutput;
    }),
  );
}

interface PackageJson {
  name: string;
  version: string;
  license?: string;
  author?: string | { name?: string };
  dependencies?: Record<string, string>;
}

function readPackage(dir: string): PackageJson {
  return JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')) as PackageJson;
}

/** The folder of package `name` as `from` sees it: its own node_modules, then the root. */
function packageDir(name: string, from: string): string {
  // As Node resolves: the `node_modules` of the package, then those of each folder above it. A
  // package of bun keeps its dependencies beside it, in the folder above its real path.
  for (let dir = realpathSync(from); ; dir = dirname(dir)) {
    const candidate = join(dir, 'node_modules', name);
    if (existsSync(join(candidate, 'package.json'))) return candidate;
    if (dirname(dir) === dir) break;
  }
  throw new Error(`cannot find the package ${name} from ${from}: run bun install`);
}

/** The text of the MIT license, for a package that names it and ships no license file. */
const MIT = `Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;

/**
 * The license files of a package, sorted by name. A package with the MIT license and no file
 * (`react-remove-scroll-bar`, a dependency of Radix) gets the text of the license with its author.
 * Any other package without a file is an error.
 */
function licenseTexts(dir: string): string[] {
  const files = readdirSync(dir)
    .filter((file) => /^(licen[cs]e|copying)/i.test(file) && !file.endsWith('.spdx'))
    .sort();
  if (files.length > 0) return files.map((file) => clean(readFileSync(join(dir, file), 'utf8')));
  const pkg = readPackage(dir);
  const author = typeof pkg.author === 'string' ? pkg.author : pkg.author?.name;
  if (pkg.license !== 'MIT' || !author) {
    throw new Error(`the package in ${dir} has no license file`);
  }
  const holder = author.replace(/\s*<[^>]*>/, '').replace(/\s+/g, ' ');
  return [`MIT License\n\nCopyright (c) ${holder}\n\n${MIT}`];
}

/** The text of a file up to the first line that starts with `stop`. */
function upTo(text: string, stop: string): string {
  const at = text.indexOf(`\n${stop}`);
  if (at === -1) throw new Error(`the line "${stop}" is missing`);
  return clean(text.slice(0, at));
}

/** The packages in `dependencies` of the apps, through the workspace packages and their own dependencies. */
function jsItems(): Item[] {
  const workspaces = new Map<string, string>();
  for (const parent of ['packages', 'apps']) {
    for (const entry of readdirSync(join(root, parent))) {
      const dir = join(root, parent, entry);
      if (existsSync(join(dir, 'package.json'))) workspaces.set(readPackage(dir).name, dir);
    }
  }
  const found = new Map<string, string>();
  const seen = new Set<string>();
  const walk = (dir: string) => {
    if (seen.has(dir)) return;
    seen.add(dir);
    for (const [name, range] of Object.entries(readPackage(dir).dependencies ?? {})) {
      const local = workspaces.get(name);
      if (range.startsWith('workspace:') && local) {
        walk(local);
      } else if (!found.has(name)) {
        // A dependency of a dependency goes into the bundle too, such as `scheduler` of
        // `react-dom`.
        const external = packageDir(name, dir);
        found.set(name, external);
        walk(external);
      }
    }
  };
  for (const app of JS_APPS) walk(join(root, app));

  const items: Item[] = [...found.entries()]
    .sort((a, b) => compare(a[0], b[0]))
    .map(([name, dir]) => {
      const pkg = readPackage(dir);
      return {
        title: `${name} ${pkg.version}`,
        license: pkg.license ?? 'see the text',
        what: `A dependency of the webview and of the phone app.`,
        texts: licenseTexts(dir),
      };
    });

  const tauri = found.get('@tauri-apps/api');
  if (!tauri) throw new Error('@tauri-apps/api is not a dependency of the apps any more');
  const tslib = clean(readFileSync(join(tauri, 'external/tslib/tslib.es6.js'), 'utf8'));
  const header = /^\/\*+\n([\s\S]*?)\n\*+ \*\//.exec(tslib);
  if (!header?.[1])
    throw new Error('the license header of the tslib copy in @tauri-apps/api is missing');
  items.push({
    title: 'tslib',
    license: '0BSD',
    what: '@tauri-apps/api copies helpers of tslib into its code (`external/tslib`).',
    texts: [clean(header[1])],
  });

  const vite = packageDir('vite', join(root, 'apps/desktop'));
  items.push({
    title: `vite ${readPackage(vite).version}`,
    license: 'MIT',
    what: 'Vite puts its module preload code into the bundles. Only the license of Vite itself applies.',
    texts: [
      upTo(readFileSync(join(vite, 'LICENSE.md'), 'utf8'), '# Licenses of bundled dependencies'),
    ],
  });
  return items;
}

function dataItems(): Item[] {
  const icons = join(root, 'node_modules/material-icon-theme');
  const syntect = /\nname = "syntect"\nversion = "([^"]+)"/.exec(
    readFileSync(join(root, 'Cargo.lock'), 'utf8'),
  );
  if (!syntect?.[1]) throw new Error('Cargo.lock has no syntect');
  return [
    {
      title: `material-icon-theme ${readPackage(icons).version}`,
      license: 'MIT',
      what:
        'The icons of the Files tab and the tables of their names come from this package ' +
        '(`packages/ui/src/files/icon-art.ts` and `crates/nh-core/src/files/icons.rs`).',
      texts: licenseTexts(icons),
    },
    {
      title: `The syntax definitions of syntect ${syntect[1]}`,
      license: 'see the text',
      what:
        'syntect bundles syntax definitions from https://github.com/sublimehq/Packages. ' +
        'This is the license of that repository.',
      texts: [SUBLIME_PACKAGES_LICENSE],
    },
  ];
}

if (import.meta.main) {
  const check = process.argv.includes('--check');
  const text = render(mergeRust(await aboutAll()), jsItems(), dataItems());
  const now = existsSync(target) ? readFileSync(target, 'utf8') : '';
  if (check) {
    if (now !== text) {
      console.error('THIRD_PARTY_LICENSES.md is out of date: run `mise run gen:notices`');
      process.exit(1);
    }
    console.log('THIRD_PARTY_LICENSES.md is up to date');
  } else if (now === text) {
    console.log('THIRD_PARTY_LICENSES.md is up to date');
  } else {
    writeFileSync(target, text);
    console.log(`wrote THIRD_PARTY_LICENSES.md (${text.length} bytes)`);
  }
}
