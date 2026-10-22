// Writes the icons of the Files tab from the package `material-icon-theme` (MIT).
//
//   bun scripts/gen-file-icons.ts    `mise run gen:icons`
//
// The core picks the icon of an entry by its name, so it gets the tables of names:
// `crates/nh-core/src/files/icons.rs`. The page only draws the icon, so it gets the art of each
// icon: `packages/ui/src/files/icon-art.ts`. Both come from the icons that FILES and FOLDERS name
// here, and only from them, so that the bundle stays small (`mise run check:bundle`). A name of the
// theme whose icon is not here falls through to the next rule of the core (an extension, then the
// icon of any file or folder).
//
// The script reads an icon only when it is paths with a fill and nothing else (no group, gradient,
// stroke or shape), and it keeps only the data of each path, its fill and its fill rule. A folder of
// the theme is the shape of a folder in one colour with a motive over it. When the closed and the
// open icon have the same motive over the two standard shapes, the art keeps the colour and the
// motive once.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { format, resolveConfig } from 'prettier';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = join(root, 'node_modules/material-icon-theme');
const rustTarget = join(root, 'crates/nh-core/src/files/icons.rs');
const tsTarget = join(root, 'packages/ui/src/files/icon-art.ts');

/** The icons of files. `file` is the icon of any other file. */
export const FILES = [
  'file',
  // Code.
  'typescript',
  'typescript-def',
  'javascript',
  'react',
  'react_ts',
  'test-ts',
  'test-js',
  'test-jsx',
  'rust',
  'python',
  'go',
  'go-mod',
  'java',
  'swift',
  'c',
  'h',
  'cpp',
  'hpp',
  'csharp',
  'objective-c',
  'ruby',
  'php',
  'lua',
  'zig',
  'dart',
  'elixir',
  'erlang',
  'haskell',
  'scala',
  'r',
  'fsharp',
  'assembly',
  'powershell',
  'console',
  'vue',
  'svelte',
  'astro',
  'html',
  'css',
  'sass',
  'less',
  'postcss',
  'tailwindcss',
  'graphql',
  'proto',
  'prisma',
  'webassembly',
  'shader',
  'jupyter',
  'tex',
  // Data and configuration.
  'json',
  'yaml',
  'toml',
  'xml',
  'markdown',
  'mdx',
  'database',
  'settings',
  'tune',
  'lock',
  'log',
  'table',
  'certificate',
  'key',
  'diff',
  'http',
  'mermaid',
  // Documents and media.
  'document',
  'pdf',
  'word',
  'powerpoint',
  'image',
  'svg',
  'video',
  'audio',
  'font',
  'zip',
  'exe',
  'email',
  'subtitles',
  '3d',
  // Projects and tools.
  'readme',
  'changelog',
  'license',
  'contributing',
  'authors',
  'conduct',
  'todo',
  'agent',
  'git',
  'docker',
  'makefile',
  'cmake',
  'nodejs',
  'npm',
  'pnpm',
  'yarn',
  'bun',
  'deno',
  'tsconfig',
  'vite',
  'vitest',
  'jest',
  'playwright',
  'storybook',
  'eslint',
  'prettier',
  'biome',
  'rollup',
  'esbuild',
  'commitlint',
  'lintstaged',
  'tauri',
  'github-actions-workflow',
  'terraform',
  'nginx',
  'gradle',
  'android',
  'angular',
  'nuxt',
  'drizzle',
  'netlify',
  'renovate',
  'robots',
  'favicon',
  'vim',
] as const;

/** The icons of folders. `folder` is the icon of any other folder. */
export const FOLDERS = [
  'folder',
  'folder-src',
  'folder-lib',
  'folder-test',
  'folder-docs',
  'folder-scripts',
  'folder-node',
  'folder-github',
  'folder-vscode',
  'folder-claude',
  'folder-dist',
  'folder-target',
  'folder-public',
  'folder-resource',
  'folder-images',
  'folder-components',
  'folder-packages',
  'folder-app',
  'folder-tools',
  'folder-config',
  'folder-migrations',
  'folder-sass',
  'folder-utils',
  'folder-helper',
  'folder-typescript',
  'folder-javascript',
  'folder-python',
  'folder-rust',
  'folder-hook',
  'folder-api',
  'folder-server',
  'folder-client',
  'folder-examples',
  'folder-mock',
  'folder-benchmark',
  'folder-store',
  'folder-layout',
  'folder-input',
  'folder-tasks',
  'folder-mobile',
  'folder-theme',
  'folder-delta',
  'folder-robot',
  'folder-src-tauri',
  'folder-command',
  'folder-temp',
  'folder-log',
  'folder-coverage',
  'folder-database',
  'folder-views',
  'folder-i18n',
  'folder-plugin',
  'folder-git',
  'folder-docker',
  'folder-secure',
  'folder-keys',
  'folder-shared',
  'folder-middleware',
  'folder-routes',
  'folder-context',
  'folder-interface',
  'folder-content',
  'folder-font',
  'folder-video',
  'folder-audio',
  'folder-svg',
  'folder-markdown',
  'folder-template',
  'folder-core',
  'folder-ui',
  'folder-environment',
  'folder-private',
  'folder-functions',
  'folder-include',
  'folder-controller',
  'folder-ci',
  'folder-ios',
  'folder-android',
  'folder-json',
  'folder-archive',
  'folder-generator',
  'folder-messages',
  'folder-backup',
  'folder-other',
  'folder-project',
  'folder-home',
  'folder-download',
  'folder-container',
  'folder-proto',
  'folder-macos',
  'folder-linux',
  'folder-windows',
  'folder-review',
  'folder-error',
  'folder-rules',
  'folder-admin',
  'folder-class',
  'folder-constant',
  'folder-debug',
  'folder-import',
  'folder-export',
  'folder-animation',
  'folder-shader',
  'folder-terraform',
  'folder-gradle',
] as const;

interface Manifest {
  iconDefinitions: Record<string, { iconPath: string }>;
  fileNames: Record<string, string>;
  fileExtensions: Record<string, string>;
  folderNames: Record<string, string>;
}

/** A path of an icon: its fill, its data, and 1 when it fills by the even-odd rule. */
type IconPath = [fill: string, d: string, evenOdd?: 1];
interface Icon {
  viewBox: string;
  paths: IconPath[];
}

/** The attributes that each element may have. The ones that change no fill are ignored. */
const ALLOWED: Record<string, ReadonlySet<string>> = {
  svg: new Set([
    'xmlns',
    'xmlns:xlink',
    'viewBox',
    'xml:space',
    'fill',
    'fill-rule',
    'clip-rule',
    'width',
    'height',
    'image-rendering',
    'shape-rendering',
    'text-rendering',
    'stroke-linejoin',
    'stroke-miterlimit',
  ]),
  g: new Set(['id', 'fill', 'fill-rule', 'clip-rule', 'stroke-width', 'data-mit-no-recolor']),
  path: new Set(['id', 'd', 'fill', 'fill-rule', 'clip-rule', 'data-mit-no-recolor']),
  circle: new Set(['id', 'cx', 'cy', 'r', 'fill']),
  ellipse: new Set(['id', 'cx', 'cy', 'rx', 'ry', 'fill']),
  rect: new Set(['id', 'x', 'y', 'width', 'height', 'rx', 'ry', 'fill']),
};
const FILL = /^(#[0-9a-fA-F]{3,8}|[a-z]+)$/;
const DATA = /^[0-9a-zA-Z .,+-]*$/;
const NUMBER = /^-?[0-9]*\.?[0-9]+$/;
const VIEW_BOX = /^-?[0-9.]+ -?[0-9.]+ [0-9.]+ [0-9.]+$/;

function attributes(text: string): Map<string, string> {
  return new Map([...text.matchAll(/([\w:-]+)="([^"]*)"/g)].map((match) => [match[1]!, match[2]!]));
}

/** The data of a path for a circle, an ellipse or a rectangle, or null when a value is not a number. */
function shapeData(tag: string, attrs: Map<string, string>): string | null {
  const values = new Map<string, number>();
  for (const [name, value] of attrs) {
    if (name === 'id' || name === 'fill') continue;
    if (!NUMBER.test(value)) return null;
    values.set(name, Number(value));
  }
  const at = (name: string): number => values.get(name) ?? 0;
  if (tag === 'circle' || tag === 'ellipse') {
    const rx = tag === 'circle' ? at('r') : at('rx');
    const ry = tag === 'circle' ? at('r') : at('ry');
    return `M${at('cx') - rx} ${at('cy')}a${rx} ${ry} 0 1 0 ${2 * rx} 0a${rx} ${ry} 0 1 0 ${-2 * rx} 0z`;
  }
  const [x, y, w, h] = [at('x'), at('y'), at('width'), at('height')];
  const rx = Math.min(values.get('rx') ?? values.get('ry') ?? 0, w / 2);
  const ry = Math.min(values.get('ry') ?? values.get('rx') ?? 0, h / 2);
  if (rx === 0 || ry === 0) return `M${x} ${y}h${w}v${h}h${-w}z`;
  const arc = (dx: number, dy: number): string => `a${rx} ${ry} 0 0 1 ${dx} ${dy}`;
  return (
    `M${x + rx} ${y}h${w - 2 * rx}${arc(rx, ry)}v${h - 2 * ry}${arc(-rx, ry)}` +
    `h${2 * rx - w}${arc(-rx, -ry)}v${2 * ry - h}${arc(rx, -ry)}z`
  );
}

/** The paths of an SVG of the theme, or the reason that this script does not read it. */
export function readIcon(svg: string): Icon | string {
  const tags = [...svg.matchAll(/<(\/?)([a-zA-Z][\w:-]*)([^>]*?)(\/?)>/g)];
  if (tags[0]?.[2] !== 'svg') return 'it does not start with <svg>';
  let viewBox = '';
  const paths: IconPath[] = [];
  // The fill and the fill rule that each open element passes to the elements in it.
  const inherited: { fill: string | undefined; evenOdd: boolean }[] = [];
  for (const [, closing, tag, rest, selfClosing] of tags) {
    if (closing) {
      inherited.pop();
      continue;
    }
    const allowed = ALLOWED[tag!];
    if (!allowed) return `it has <${tag}>`;
    const attrs = attributes(rest!);
    for (const name of attrs.keys()) {
      if (!allowed.has(name)) return `<${tag}> has ${name}`;
    }
    const parent = inherited.at(-1);
    const fill = attrs.get('fill') ?? parent?.fill;
    const rule = attrs.get('fill-rule');
    const evenOdd = rule === undefined ? (parent?.evenOdd ?? false) : rule === 'evenodd';
    if (tag === 'svg') {
      viewBox = attrs.get('viewBox') ?? '';
      if (!VIEW_BOX.test(viewBox)) return `its view box "${viewBox}" is not four numbers`;
    }
    if (tag === 'svg' || tag === 'g') {
      if (!selfClosing) inherited.push({ fill, evenOdd });
      continue;
    }
    if (!selfClosing) inherited.push({ fill, evenOdd });
    // A shape with no fill is a box that only sizes the icon.
    if (fill === 'none') continue;
    if (fill === undefined || !FILL.test(fill)) return `a <${tag}> has the fill ${fill}`;
    const d = tag === 'path' ? (attrs.get('d') ?? '').trim() : shapeData(tag!, attrs);
    if (d === null || !DATA.test(d)) return `a <${tag}> has data that is not a path`;
    paths.push(evenOdd ? [fill, d, 1] : [fill, d]);
  }
  if (paths.length === 0) return 'it has no path';
  return { viewBox, paths };
}

/** Sorts by the UTF-8 bytes of the key, as Rust compares `&str`. */
function byBytes(a: [string, string], b: [string, string]): number {
  return Buffer.compare(Buffer.from(a[0]), Buffer.from(b[0]));
}

/**
 * The entries of a table of the theme whose icon is in `icons`, with the key in lower case. The
 * core compares names without case, as the theme does. When two keys differ only in case, the
 * key that was in lower case wins.
 */
export function table(
  source: Record<string, string>,
  icons: ReadonlySet<string>,
): [string, string][] {
  const out = new Map<string, string>();
  for (const [key, icon] of Object.entries(source)) {
    if (!icons.has(icon)) continue;
    const lower = key.toLowerCase();
    if (!out.has(lower) || key === lower) out.set(lower, icon);
  }
  return [...out].sort(byBytes);
}

/** The words of `line` in lines of at most `width` characters. */
function wrap(line: string, width: number): string[] {
  const out: string[] = [];
  let current = '';
  for (const word of line.split(' ')) {
    if (current && current.length + 1 + word.length > width) {
      out.push(current);
      current = word;
    } else {
      current = current ? `${current} ${word}` : word;
    }
  }
  return [...out, current];
}

const LICENSE_NOTE = (version: string, license: string): string[] => [
  `The icons and the tables of names come from material-icon-theme ${version}`,
  '(https://github.com/material-extensions/vscode-material-icon-theme), under this license:',
  '',
  ...license
    .trim()
    .split('\n')
    .flatMap((line) => wrap(line.trim(), 94)),
];

function rustTable(name: string, doc: string, rows: [string, string][]): string {
  const lines = rows.map(([key, icon]) => `    (${JSON.stringify(key)}, ${JSON.stringify(icon)}),`);
  return `/// ${doc}\n#[rustfmt::skip]\npub(super) static ${name}: &[(&str, &str)] = &[\n${lines.join('\n')}\n];\n`;
}

function tsPaths(paths: IconPath[]): string {
  return `[${paths.map((path) => JSON.stringify(path)).join(', ')}]`;
}

async function main(): Promise<void> {
  const version = (
    JSON.parse(readFileSync(join(pkg, 'package.json'), 'utf8')) as { version: string }
  ).version;
  const license = readFileSync(join(pkg, 'LICENSE'), 'utf8');
  const manifest = JSON.parse(
    readFileSync(join(pkg, 'dist/material-icons.json'), 'utf8'),
  ) as Manifest;

  const problems: string[] = [];
  const load = (name: string): Icon | null => {
    const definition = manifest.iconDefinitions[name];
    if (!definition) {
      problems.push(`${name}: the theme has no such icon`);
      return null;
    }
    const icon = readIcon(readFileSync(join(pkg, 'dist', definition.iconPath), 'utf8'));
    if (typeof icon === 'string') {
      problems.push(`${name}: ${icon}`);
      return null;
    }
    return icon;
  };

  const files = new Map<string, Icon>();
  for (const name of FILES) {
    const icon = load(name);
    if (icon) files.set(name, icon);
  }
  const closedShape = load('folder')?.paths[0]?.[1];
  const openShape = load('folder-open')?.paths[0]?.[1];
  const folders = new Map<string, string>();
  for (const name of FOLDERS) {
    const closed = load(name);
    const open = load(`${name}-open`);
    if (!closed || !open) continue;
    const [closedFolder, ...motive] = closed.paths;
    const [openFolder, ...openMotive] = open.paths;
    const color = closedFolder?.[0];
    const standard =
      closed.viewBox === '0 0 16 16' &&
      open.viewBox === closed.viewBox &&
      closedFolder?.[1] === closedShape &&
      openFolder?.[1] === openShape &&
      openFolder?.[0] === color &&
      JSON.stringify(motive) === JSON.stringify(openMotive);
    folders.set(
      name,
      standard
        ? `[${JSON.stringify(color)}, ${tsPaths(motive)}]`
        : `[[${JSON.stringify(closed.viewBox)}, ${tsPaths(closed.paths)}], [${JSON.stringify(open.viewBox)}, ${tsPaths(open.paths)}]]`,
    );
  }
  if (problems.length > 0) {
    throw new Error(`These icons cannot be used:\n  ${problems.join('\n  ')}`);
  }

  const note = LICENSE_NOTE(version, license);
  const fileSet = new Set<string>(FILES);
  const folderSet = new Set<string>(FOLDERS);
  const rust = [
    '//! The tables of names of the icons of the Files tab. The core gives each entry of',
    '//! `fs.list` and each `fs.read` the name of an icon, and the page draws it.',
    '//!',
    '//! Generated by `scripts/gen-file-icons.ts` (`mise run gen:icons`). Do not edit by hand.',
    '//!',
    ...note.map((line) => (line ? `//! ${line}` : '//!')),
    '',
    rustTable(
      'FILE_NAMES',
      'Whole names of files, in lower case, and their icons, by the bytes of the name.',
      table(manifest.fileNames, fileSet),
    ),
    rustTable(
      'FILE_EXTENSIONS',
      'Extensions of files without the first dot, in lower case, and their icons.',
      table(manifest.fileExtensions, fileSet),
    ),
    rustTable(
      'FOLDER_NAMES',
      'Names of folders, in lower case, and their icons.',
      table(manifest.folderNames, folderSet),
    ),
  ].join('\n');
  writeFileSync(rustTarget, rust);

  const ts = [
    '// The art of the icons of the Files tab. The core sends the name of an icon, and',
    '// `FileIcon.tsx` draws its art from here.',
    '//',
    '// Generated by scripts/gen-file-icons.ts (`mise run gen:icons`). Do not edit by hand.',
    '//',
    ...note.map((line) => (line ? `// ${line}` : '//')),
    '',
    '/** A path of an icon: its fill, its data, and 1 when it fills by the even-odd rule. */',
    'export type IconPath = readonly [fill: string, d: string, evenOdd?: 1];',
    '',
    '/** An icon: its view box and its paths. */',
    'export type IconArt = readonly [viewBox: string, paths: readonly IconPath[]];',
    '',
    '/**',
    ' * A folder: the colour of the shape of the folder and the paths over it, on a view box of 16, or',
    ' * the whole art of the closed folder and of the open folder.',
    ' */',
    'export type FolderArt =',
    '  | readonly [color: string, motive: readonly IconPath[]]',
    '  | readonly [closed: IconArt, open: IconArt];',
    '',
    '/** The shape of a closed folder, on a view box of 16. */',
    `export const CLOSED_FOLDER = ${JSON.stringify(closedShape)};`,
    '',
    '/** The shape of an open folder, on a view box of 16. */',
    `export const OPEN_FOLDER = ${JSON.stringify(openShape)};`,
    '',
    'export const FILE_ART: Readonly<Record<string, IconArt>> = {',
    ...[...files].map(
      ([name, icon]) =>
        `  ${JSON.stringify(name)}: [${JSON.stringify(icon.viewBox)}, ${tsPaths(icon.paths)}],`,
    ),
    '};',
    '',
    'export const FOLDER_ART: Readonly<Record<string, FolderArt>> = {',
    ...[...folders].map(([name, art]) => `  ${JSON.stringify(name)}: ${art},`),
    '};',
    '',
  ].join('\n');
  const config = (await resolveConfig(tsTarget)) ?? {};
  writeFileSync(tsTarget, await format(ts, { ...config, parser: 'typescript' }));
  console.log(
    `${files.size} file icons and ${folders.size} folder icons from material-icon-theme ${version}`,
  );
}

if (import.meta.main) await main();
