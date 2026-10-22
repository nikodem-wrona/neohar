// Writes the reference of every configuration key from the compiled-in defaults of the core.
//
// `crates/nh-core/src/config/defaults.toml` is the only source of defaults: a key that is
// not in it is reported and ignored. Its comments are the descriptions on the page.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { anchor, cell, docsRoot, frontMatter, generatedNote, repoRoot, write } from './lib';
import { prose } from './markdown';

const SOURCE = 'crates/nh-core/src/config/defaults.toml';
const GENERATOR = 'gen-config.ts';

interface Key {
  name: string;
  value: string;
  comment: string;
}

interface Table {
  name: string;
  array: boolean;
  comment: string;
  keys: Key[];
}

function parse(text: string): { header: string; tables: Table[] } {
  const tables: Table[] = [];
  let comment: string[] = [];
  let header = '';
  let current: Table | undefined;
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line.startsWith('#')) {
      comment.push(line.replace(/^#\s?/, ''));
      continue;
    }
    if (!line) {
      if (!current && comment.length) {
        header = comment.join('\n');
        comment = [];
      }
      continue;
    }
    const table = /^\[(\[)?([^\]]+)\]\]?$/.exec(line);
    if (table?.[2]) {
      // A comment right above a header describes the table.
      current = { name: table[2], array: Boolean(table[1]), comment: comment.join(' '), keys: [] };
      comment = [];
      tables.push(current);
      continue;
    }
    const key = /^([A-Za-z0-9_-]+)\s*=\s*(.*)$/.exec(line);
    if (key?.[1] && current) {
      current.keys.push({ name: key[1], value: key[2] ?? '', comment: comment.join(' ') });
      comment = [];
    }
  }
  return { header, tables };
}

export function generateConfig(): void {
  const text = readFileSync(join(repoRoot, SOURCE), 'utf8');
  const { header, tables } = parse(text);
  const lines: string[] = [
    frontMatter({
      title: 'Default values',
      description: 'Every configuration key of neohar with its default value and meaning.',
      sidebar_position: 2,
    }),
    generatedNote(SOURCE, GENERATOR),
    prose(header.replace(/\n/g, ' ')),
    '',
    'Write the keys that you want to change in `~/.config/neohar/config.toml` (or `$NEOHAR_CONFIG_DIR/config.toml`), under the same table names. See [Configuration](/docs/reference/configuration) for how the file is merged, reloaded and edited.',
    '',
    '| Table | Keys |',
    '|---|---|',
    ...tables.map(
      (t) =>
        `| [\`[${t.name}]\`](#${anchor(t.name)}) | ${t.keys.map((k) => `\`${k.name}\``).join(', ') || '—'} |`,
    ),
    '',
  ];
  for (const t of tables) {
    lines.push(`## \`[${t.name}]\` {#${anchor(t.name)}}`, '');
    if (t.comment) lines.push(prose(t.comment), '');
    if (t.keys.length) {
      lines.push(
        '| Key | Default | Description |',
        '|---|---|---|',
        ...t.keys.map(
          (k) => `| \`${k.name}\` | \`${cell(k.value)}\` | ${cell(prose(k.comment))} |`,
        ),
        '',
      );
    } else {
      lines.push('This table has no keys by default.', '');
    }
    lines.push('```toml', `[${t.name}]`, ...t.keys.map((k) => `${k.name} = ${k.value}`), '```', '');
  }
  lines.push('## The whole file', '', '```toml', text.trimEnd(), '```');
  write(join(docsRoot, 'reference/configuration/defaults.md'), lines.join('\n'));
  console.log(
    `config: ${tables.length} tables, ${tables.reduce((n, t) => n + t.keys.length, 0)} keys`,
  );
}

if (import.meta.main) generateConfig();
