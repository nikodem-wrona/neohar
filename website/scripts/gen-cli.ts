// Writes one page for each noun of the `nh` command line from the help of every command.
//
// It runs the newest of `target/debug/nh` and `target/release/nh` with `--help` only, which reads
// nothing and talks to no core. Without a binary it keeps the pages that are there. With `--check`,
// it also skips the pages when the binary is older than a source file of `crates/nh/src`, the only
// crate with the definitions of the commands.
import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { loadContract } from './gen-api';
import {
  anchor,
  checkOnly,
  clearGenerated,
  docsRoot,
  frontMatter,
  githubBlob,
  repoRoot,
  write,
} from './lib';

const OUT = join(docsRoot, 'reference/cli/commands');
const DATA = join(import.meta.dir, 'data/cli.json');
const GENERATOR = 'gen-cli.ts';

interface Command {
  path: string[];
  about: string;
  help: string;
  children: string[];
}

function binary(): string | undefined {
  const candidates = ['target/debug/nh', 'target/release/nh']
    .map((p) => join(repoRoot, p))
    .filter((p) => existsSync(p));
  candidates.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return candidates[0];
}

/** The time of the newest file under a directory, in milliseconds. */
function newest(dir: string): number {
  let time = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    time = Math.max(time, entry.isDirectory() ? newest(path) : statSync(path).mtimeMs);
  }
  return time;
}

function help(nh: string, path: string[]): string {
  const run = spawnSync(nh, [...path, '--help'], {
    encoding: 'utf8',
    env: { PATH: process.env.PATH ?? '', NO_COLOR: '1', TERM: 'dumb', COLUMNS: '100' },
    timeout: 10_000,
  });
  if (run.status !== 0) throw new Error(`nh ${path.join(' ')} --help: ${run.stderr}`);
  return run.stdout.trimEnd();
}

/** The subcommands in the `Commands:` part of a help text. */
function subcommands(text: string): { name: string; about: string }[] {
  const lines = text.split('\n');
  const start = lines.findIndex((l) => /^Commands:\s*$/.test(l));
  if (start < 0) return [];
  const result: { name: string; about: string }[] = [];
  for (const line of lines.slice(start + 1)) {
    if (!line.trim()) break;
    const m = /^ {2}(\S+)\s{2,}(.*)$/.exec(line) ?? /^ {2}(\S+)\s*$/.exec(line);
    if (m?.[1]) {
      if (m[1] !== 'help') result.push({ name: m[1], about: m[2] ?? '' });
    } else if (result.length && /^\s{6,}\S/.test(line)) {
      const last = result[result.length - 1];
      if (last) last.about += ` ${line.trim()}`;
    }
  }
  return result;
}

function walk(nh: string, path: string[], about: string, out: Command[]): void {
  const text = help(nh, path);
  const children = subcommands(text);
  out.push({ path, about, help: text, children: children.map((c) => c.name) });
  for (const child of children) walk(nh, [...path, child.name], child.about, out);
}

let methods: Set<string> | undefined;

/** The API method that a command calls, when its name is the noun and the verb of a method. */
function methodFor(path: string[]): string | undefined {
  methods ??= new Set(loadContract().methods.map((m) => m.name));
  if (path.length !== 2) return undefined;
  const name = `${path[0]}.${(path[1] ?? '').replace(/-/g, '_')}`;
  return methods.has(name) ? name : undefined;
}

function fence(text: string): string {
  return ['```text', text.replace(/```/g, "'''"), '```'].join('\n');
}

export function generateCli(): void {
  const nh = binary();
  if (!nh) {
    console.log('cli: no nh binary in target/, the pages stay as they are');
    return;
  }
  if (checkOnly && statSync(nh).mtimeMs < newest(join(repoRoot, 'crates/nh/src'))) {
    console.log(
      `cli: ${nh.replace(`${repoRoot}/`, '')} is older than crates/nh/src, so the CLI pages are not checked. Run mise run build:nh first`,
    );
    return;
  }
  const commands: Command[] = [];
  walk(nh, [], '', commands);
  const root = commands[0];
  if (!root) return;
  write(
    DATA,
    JSON.stringify(
      commands.map((c) => ({ path: c.path, about: c.about })),
      null,
      2,
    ),
  );

  clearGenerated(OUT);
  const nouns = commands.filter((c) => c.path.length === 1);
  write(
    join(OUT, 'index.md'),
    [
      frontMatter({
        title: 'Commands',
        description: 'Every command of nh, from its own help.',
      }),
      note(nh),
      'Every command below is `nh <noun> <verb>`. The global options (`--session`, `--socket`, `--json`, `--timeout`) work with every command; see [Command line (nh)](/docs/reference/cli).',
      '',
      fence(root.help),
      '',
      '| Command | What it does | Verbs |',
      '|---|---|---|',
      ...nouns.map(
        (n) =>
          `| [\`nh ${n.path[0]}\`](/docs/reference/cli/commands/${n.path[0]}) | ${n.about.replace(/\|/g, '\\|')} | ${n.children.map((c) => `\`${c}\``).join(', ')} |`,
      ),
    ].join('\n'),
  );

  nouns.forEach((noun, position) => {
    const name = noun.path[0] ?? '';
    const below = commands.filter((c) => c.path[0] === name && c.path.length > 1);
    const lines: string[] = [
      frontMatter({
        title: `nh ${name}`,
        description: noun.about || `The ${name} commands of nh.`,
        sidebar_position: position + 1,
      }),
      note(nh),
      noun.about,
      '',
      fence(noun.help),
      '',
    ];
    for (const c of below) {
      const full = c.path.join(' ');
      const depth = Math.min(c.path.length, 4);
      lines.push(`${'#'.repeat(depth)} \`nh ${full}\` {#${anchor(full)}}`, '');
      if (c.about) lines.push(c.about, '');
      const method = methodFor(c.path);
      if (method) {
        const ns = method.split('.')[0] ?? '';
        lines.push(
          `**API method:** [\`${method}\`](/docs/reference/api/methods/${ns}#${anchor(method)})`,
          '',
        );
      }
      lines.push(fence(c.help), '');
    }
    write(join(OUT, `${name}.md`), lines.join('\n'));
  });
  console.log(`cli: ${commands.length} commands from ${nh.replace(`${repoRoot}/`, '')}`);
}

function note(nh: string): string {
  const which = nh.replace(`${repoRoot}/`, '');
  return [
    ':::info[Generated page]',
    '',
    `This page is generated from the \`--help\` output of \`${which}\` by \`website/scripts/${GENERATOR}\`. The commands are defined in [\`crates/nh/src/cli\`](${githubBlob.replace('/blob/', '/tree/')}/crates/nh/src/cli). Do not edit it by hand: build \`nh\` and run \`bun run gen\` in \`website/\`.`,
    '',
    ':::',
    '',
  ].join('\n');
}

if (import.meta.main) generateCli();
