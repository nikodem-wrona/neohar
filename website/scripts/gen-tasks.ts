// Writes the list of every mise task of the repository from `mise.toml`.
//
// Each task shows its description, the comment above it in `mise.toml`, the note that AGENTS.md
// gives it (for example "local, release, about 30 min"), its dependencies, directory and commands.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { anchor, cell, docsRoot, frontMatter, generatedNote, repoRoot, write } from './lib';
import { prose } from './markdown';

const SOURCE = 'mise.toml';
const GENERATOR = 'gen-tasks.ts';

interface Task {
  description?: string;
  run?: string | string[];
  depends?: string | string[];
  dir?: string;
  env?: Record<string, string>;
}

const groups: [string, string, (name: string) => boolean][] = [
  [
    'Checks',
    'Static checks. `mise run check` runs them all.',
    (n) => n === 'check' || n.startsWith('check:') || n === 'fmt',
  ],
  [
    'Tests',
    'Test suites. `mise run test` runs the Rust and UI tests; the others are local suites.',
    (n) => n === 'test' || n.startsWith('test:'),
  ],
  [
    'Benchmarks',
    'The benches of the VT engine against their budgets. They are local: CI does not run them.',
    (n) => n === 'bench' || n.startsWith('bench:'),
  ],
  ['Chaos', 'The restart runs under `SIGKILL`.', (n) => n === 'chaos' || n.startsWith('chaos:')],
  [
    'Build and run',
    'Building the WASM decoder, the sidecar and the app, and running the app in development.',
    (n) => n.startsWith('build:') || n.startsWith('app:'),
  ],
  [
    'Generation and recording',
    'Generated code and recorded harness sessions.',
    (n) => n.startsWith('gen:') || n.startsWith('record:'),
  ],
  ['Setup', 'One-time setup.', (n) => n.startsWith('setup:')],
];

/** The comment lines right above each `[tasks.…]` header. */
function comments(text: string): Map<string, string> {
  const map = new Map<string, string>();
  let pending: string[] = [];
  for (const line of text.split('\n')) {
    if (line.startsWith('#')) {
      pending.push(line.replace(/^#\s?/, ''));
      continue;
    }
    const m = /^\[tasks\.(?:"([^"]+)"|([A-Za-z0-9_-]+))\]/.exec(line);
    if (m) map.set(m[1] ?? m[2] ?? '', pending.join(' '));
    pending = [];
  }
  return map;
}

/** The notes of AGENTS.md: `mise run name  # note`. */
function agentNotes(): Map<string, string> {
  const map = new Map<string, string>();
  const text = readFileSync(join(repoRoot, 'AGENTS.md'), 'utf8');
  for (const m of text.matchAll(/^mise run ([a-z0-9:-]+)\s+#\s*(.*)$/gm)) {
    if (m[1] && m[2]) map.set(m[1], m[2]);
  }
  return map;
}

const list = (v: string | string[] | undefined) =>
  v === undefined ? [] : Array.isArray(v) ? v : [v];

export function generateTasks(): void {
  const text = readFileSync(join(repoRoot, SOURCE), 'utf8');
  const parsed = Bun.TOML.parse(text) as {
    tasks?: Record<string, Task>;
    tools?: Record<string, unknown>;
  };
  const tasks = parsed.tasks ?? {};
  const above = comments(text);
  const notes = agentNotes();
  const names = Object.keys(tasks);
  const placed = new Set<string>();

  const lines: string[] = [
    frontMatter({
      title: 'mise tasks',
      description: `All ${names.length} tasks of mise.toml: what each one does, what it depends on and the commands it runs.`,
      sidebar_position: 6,
      toc_max_heading_level: 3,
    }),
    generatedNote(SOURCE, GENERATOR),
    'Run every task from the root of the repository with `mise run <task>`. Arguments after `--` go to the last command of the task, for example `mise run app:install -- --yes`. The CI workflow runs `mise run check` and `mise run test` on macOS. A task that its description marks as local is not part of `mise run test` and needs more than a checkout, for example a browser, a GPU or an installed harness. CI does not run it.',
    '',
    '## Toolchain',
    '',
    'mise installs these tools at these versions (`mise install`):',
    '',
    '| Tool | Version |',
    '|---|---|',
    ...Object.entries(parsed.tools ?? {}).map(([tool, v]) => {
      const version =
        typeof v === 'string'
          ? v
          : String((v as { version?: string }).version ?? JSON.stringify(v));
      return `| \`${tool}\` | ${cell(version)} |`;
    }),
    '',
  ];

  const sections: string[] = [];
  const summary: string[] = ['## Summary', '', '| Task | What it does | Note |', '|---|---|---|'];
  for (const [title, intro, test] of groups) {
    const members = names.filter((n) => test(n) && !placed.has(n));
    if (!members.length) continue;
    members.forEach((n) => placed.add(n));
    sections.push(`## ${title}`, '', prose(intro), '');
    for (const name of members) {
      summary.push(
        `| [\`${name}\`](#${anchor(name)}) | ${cell(prose(tasks[name]?.description ?? ''))} | ${cell(notes.get(name) ?? '')} |`,
      );
      sections.push(...taskSection(name, tasks[name] ?? {}, above.get(name), notes.get(name)));
    }
  }
  const rest = names.filter((n) => !placed.has(n));
  if (rest.length) {
    sections.push('## Other tasks', '');
    for (const name of rest) {
      summary.push(
        `| [\`${name}\`](#${anchor(name)}) | ${cell(prose(tasks[name]?.description ?? ''))} | ${cell(notes.get(name) ?? '')} |`,
      );
      sections.push(...taskSection(name, tasks[name] ?? {}, above.get(name), notes.get(name)));
    }
  }
  lines.push(...summary, '', ...sections);
  write(join(docsRoot, 'getting-started/mise-tasks.md'), lines.join('\n'));
  console.log(`tasks: ${names.length}`);
}

function taskSection(
  name: string,
  task: Task,
  comment: string | undefined,
  note: string | undefined,
): string[] {
  const out: string[] = [`### \`${name}\` {#${anchor(name)}}`, ''];
  if (task.description) out.push(prose(task.description), '');
  if (comment) out.push(prose(comment), '');
  if (note) out.push(`**AGENTS.md:** ${prose(note)}`, '');
  const facts: string[] = [];
  const depends = list(task.depends);
  if (depends.length)
    facts.push(`**Depends on:** ${depends.map((d) => `[\`${d}\`](#${anchor(d)})`).join(', ')}`);
  if (task.dir) facts.push(`**Directory:** \`${task.dir}\``);
  if (task.env)
    facts.push(
      `**Environment:** ${Object.entries(task.env)
        .map(([k, v]) => `\`${k}=${v}\``)
        .join(', ')}`,
    );
  if (facts.length) out.push(facts.join(' · '), '');
  const run = list(task.run);
  if (run.length) out.push('```bash', ...run, '```', '');
  else if (depends.length) out.push('It runs only its dependencies.', '');
  return out;
}

if (import.meta.main) generateTasks();
