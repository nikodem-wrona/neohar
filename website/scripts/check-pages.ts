// Checks the pages of the guide for faults that the Docusaurus build does not find:
// - a `<` before a letter outside code, which CommonMark reads as an HTML tag and hides;
// - a link to a file or a directory of the repository on GitHub that does not exist;
// - a page without `title` or `description` in its front matter;
// - `Neohar` with a capital letter (the name is always lowercase);
// - a name that the repository does not have: an API method (`pane.split`), an `nh` command,
//   a `mise run` task.
//
// Run it with `bun run check`. It exits with 1 when it finds a fault.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { loadContract } from './gen-api';
import { docsRoot, repoRoot } from './lib';

const GITHUB = /https:\/\/github\.com\/nikodem-wrona\/neohar\/(blob|tree)\/main\/([^)\s#>]+)/g;

function pages(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...pages(path));
    else if (name.endsWith('.md') || name.endsWith('.mdx')) out.push(path);
  }
  return out;
}

/** The lines of a page, each marked as inside a fenced code block or not. */
function lines(text: string): { line: number; text: string; fenced: boolean }[] {
  const out: { line: number; text: string; fenced: boolean }[] = [];
  let fence: string | undefined;
  let language = '';
  text.split('\n').forEach((raw, i) => {
    const marker = /^\s*(`{3,}|~{3,})\s*(\w*)/.exec(raw);
    if (marker?.[1]) {
      if (!fence) {
        fence = marker[1];
        language = marker[2] ?? '';
      } else if (marker[1].startsWith(fence)) fence = undefined;
      return;
    }
    // A diagram is not text that a person reads as names or commands.
    if (fence && language === 'mermaid') return;
    out.push({ line: i + 1, text: raw, fenced: Boolean(fence) });
  });
  return out;
}

const withoutCode = (text: string) => text.replace(/(`+)[\s\S]*?\1/g, '');
const codeSpans = (text: string) => [...text.matchAll(/(`+)([\s\S]*?)\1/g)].map((m) => m[2] ?? '');

// What the repository has.
const contract = loadContract();
const methods = new Set(contract.methods.map((m) => m.name));
const namespaces = new Set(contract.methods.map((m) => m.name.split('.')[0] ?? ''));
const known = new Set<string>([
  ...methods,
  ...contract.events.map((e) => e.kind),
  ...contract.notifications.map((n) => n.name),
  ...contract.streams.map((s) => s.kind),
  ...contract.projections.map((p) => p.name),
]);
// Dotted names that the code itself uses as strings: dev-only methods, palette commands, keys.
const sourceNames = (() => {
  const names = new Set<string>();
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      if (name === 'node_modules' || name === 'target' || name === 'pkg') continue;
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (/\.(rs|ts|tsx|toml)$/.test(name)) {
        for (const m of readFileSync(path, 'utf8').matchAll(/["'`]([a-z]+\.[a-z_]+)["'`:]/g)) {
          if (m[1]) names.add(m[1]);
        }
      }
    }
  };
  for (const dir of ['crates', 'apps/desktop/src-tauri/src', 'packages', 'tools'])
    walk(join(repoRoot, dir));
  // The configuration keys: `table.key` for every key of the defaults.
  const defaults = readFileSync(join(repoRoot, 'crates/nh-core/src/config/defaults.toml'), 'utf8');
  let table = '';
  for (const line of defaults.split('\n')) {
    const t = /^\[([^\]]+)\]/.exec(line);
    if (t?.[1]) table = t[1];
    const k = /^([a-z_]+)\s*=/.exec(line);
    if (k?.[1]) names.add(`${table}.${k[1]}`);
  }
  return names;
})();
const fileName = /\.(rs|toml|ts|tsx|js|json|md|sh|db|html|css|log|txt|yml|yaml|lock)$/;
const cliPath = join(import.meta.dir, 'data/cli.json');
const commands = existsSync(cliPath)
  ? new Set(
      (JSON.parse(readFileSync(cliPath, 'utf8')) as { path: string[] }[]).map((c) =>
        c.path.join(' '),
      ),
    )
  : undefined;
// Roles of `nh` that are not commands of the CLI parser.
const roles = new Set(['hold', 'hook', 'daemon', 'bridge', 'askpass', 'help']);
// Global options of `nh` that take a value: the word after one is not the noun.
const valueOptions = new Set(['--session', '--socket', '--machine', '--timeout']);
const tasks = new Set(
  Object.keys(
    (Bun.TOML.parse(readFileSync(join(repoRoot, 'mise.toml'), 'utf8')) as { tasks: object }).tasks,
  ),
);

let faults = 0;
let warnings = 0;
const report = (file: string, line: number, message: string) => {
  faults++;
  console.log(`${relative(process.cwd(), file)}:${line}: ${message}`);
};
/** A name that the repository does not have. A page may name a planned one: read the line. */
const warn = (file: string, line: number, message: string) => {
  warnings++;
  console.log(`${relative(process.cwd(), file)}:${line}: warning: ${message}`);
};

/** Checks one `nh …` command line: the noun and, when it has one, the verb. */
function checkCommand(file: string, line: number, words: string[]): void {
  if (!commands) return;
  const args = words.filter((w, i) => !w.startsWith('-') && !valueOptions.has(words[i - 1] ?? ''));
  const noun = args[0];
  if (!noun || roles.has(noun) || /[^a-z-]/.test(noun)) return;
  if (!commands.has(noun)) {
    warn(file, line, `no such nh command: nh ${noun}`);
    return;
  }
  const verb = args[1];
  const hasVerbs = [...commands].some((c) => c.startsWith(`${noun} `));
  if (hasVerbs && verb && /^[a-z][a-z-]*$/.test(verb) && !commands.has(`${noun} ${verb}`)) {
    warn(file, line, `no such nh command: nh ${noun} ${verb}`);
  }
}

for (const file of pages(docsRoot)) {
  const text = readFileSync(file, 'utf8');
  const front = /^---\n([\s\S]*?)\n---\n/.exec(text)?.[1] ?? '';
  try {
    Bun.YAML.parse(front);
  } catch (error) {
    report(
      file,
      1,
      `the front matter is not valid YAML (quote a value that has ": "): ${String(error)}`,
    );
  }
  if (!/^title:/m.test(front)) report(file, 1, 'no title in the front matter');
  if (!/^description:/m.test(front)) report(file, 1, 'no description in the front matter');
  const generated = text.includes(':::info[Generated page]');

  for (const { line, text: raw, fenced } of lines(text)) {
    if (!fenced) {
      const prose = withoutCode(raw);
      const tag = /<([A-Za-z][\w-]*)/.exec(prose.replace(/&lt;/g, ''));
      if (tag && !/^https?$/.test(tag[1] ?? '')) {
        report(file, line, `a bare <${tag[1]}: put it in backticks`);
      }
      if (/\bNeohar\b/.test(prose)) report(file, line, 'write neohar in lowercase');
    }
    if (generated) continue;

    const spans = fenced ? [raw] : codeSpans(raw);
    for (const span of spans) {
      // API names: `namespace.verb` where the namespace is one of the API.
      if (!fenced) {
        const word = span.trim();
        const name = /^([a-z]+)\.([a-z_]+)$/.exec(word);
        if (
          name?.[1] &&
          namespaces.has(name[1]) &&
          !known.has(word) &&
          !sourceNames.has(word) &&
          !fileName.test(word)
        ) {
          warn(file, line, `no such method, event or notification: ${span.trim()}`);
        }
      }
      // `nh noun verb` command lines.
      const command = /^\s*(?:\$\s+)?nh\s+(.+)$/.exec(span);
      if (command?.[1]) checkCommand(file, line, command[1].split(/\s+/).slice(0, 3));
      // `mise run task`.
      for (const m of span.matchAll(/mise run ([a-z][a-z0-9:-]*)/g)) {
        if (m[1] && !tasks.has(m[1])) warn(file, line, `no such mise task: ${m[1]}`);
      }
    }
  }

  text.split('\n').forEach((raw, i) => {
    for (const m of raw.matchAll(GITHUB)) {
      const path = decodeURIComponent((m[2] ?? '').replace(/[.,;:]+$/, ''));
      if (!existsSync(join(repoRoot, path))) {
        report(file, i + 1, `no such path in the repository: ${path}`);
      }
    }
  });
}

if (warnings)
  console.log(`\n${warnings} warning(s): check that each page says the name is planned`);
if (faults) {
  console.log(`${faults} fault(s)`);
  process.exit(1);
}
console.log('check-pages: no faults');
