// Writes the reference of the API contract from the snapshot of `nh api schema`.
//
// The snapshot is the JSON document that `crates/nh-proto/tests/api_schema.rs` checks, so the
// pages follow the contract exactly.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import {
  anchor,
  cell,
  clearGenerated,
  docsRoot,
  frontMatter,
  generatedNote,
  githubBlob,
  repoRoot,
  write,
} from './lib';
import { paragraphs, prose } from './markdown';

const SOURCE = 'crates/nh-proto/tests/snapshots/api_schema__contract.snap';
const GENERATOR = 'gen-api.ts';
const OUT = join(docsRoot, 'reference/api');

// A JSON Schema node. The fields are read as the schema has them, so the type stays open.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Obj = { [key: string]: any };
type Schema = boolean | Obj;

interface Method {
  name: string;
  params: Schema;
  result: Schema;
  scopes: string[];
}

interface Contract {
  $defs: Record<string, Schema>;
  api_version: string;
  envelope: Record<string, Schema>;
  error_codes: { code: string; rpc_code: number }[];
  events: { kind: string; payload: Schema; v: number }[];
  methods: Method[];
  notifications: { name: string; params: Schema }[];
  projections: {
    key: string;
    name: string;
    parameter: string | null;
    singleton: boolean;
    value: Schema;
  }[];
  protocols: Record<string, number>;
  streams: { direction: string; kind: string; params: Schema }[];
}

export function loadContract(): Contract {
  const text = readFileSync(join(repoRoot, SOURCE), 'utf8');
  const end = text.indexOf('\n---\n', 3);
  if (!text.startsWith('---') || end < 0) throw new Error(`${SOURCE}: no insta header`);
  return JSON.parse(text.slice(end + 5)) as Contract;
}

/** What each namespace of methods is for. */
const namespaces: Record<string, string> = {
  session: 'The session of the core: its name, version and sockets.',
  machine: 'The machine that the core runs on.',
  workspace: 'Workspaces: groups of tabs, usually bound to a project root.',
  group: 'Groups of workspaces in the sidebar.',
  tab: 'Tabs: one layout of panes in a workspace.',
  layout: 'The split tree of a tab: ratios, equal sizes and zoom.',
  pane: 'Panes: terminals with a holder, and their input, scrolling, selection, copy and text.',
  window: 'The windows of the app: which panes they show, focus, metrics, bounds and panels.',
  render: 'Acknowledgements of painted frames, which free the slots of the render pacer.',
  agent: 'Agent runs of Claude Code and Codex: list, start, resume, prompt, interrupt, stop, wait.',
  interaction: 'What agents ask a person: approvals, questions, plan reviews and notices.',
  harness: 'Claude Code and Codex on this machine: discovery, version and login state.',
  phone:
    'The phones that supervise this machine: the offer of a pairing, names and removal, and the mapping of tailscale serve.',
  events: 'Subscriptions to the event log, from a cursor.',
  projection: 'Subscriptions to projections: a snapshot, then keyed patches.',
  config: 'The configuration: the merged values with provenance, edits and reload.',
  policy: 'Rules that decide approvals before a person is asked.',
  integration: 'The hooks that neohar installs into Claude Code and Codex.',
  doctor: 'The checks of `nh doctor`.',
  debug: 'Bug reports and tools for development builds.',
  search: 'Search of the scrollback archive.',
  palette: 'The command palette: the core ranks and runs the commands.',
  rich: 'Rich content on demand: diffs, sanitized markdown and highlighted code.',
  timeline: 'Older items of the timeline of a run.',
  jobs: 'Long commands that run as jobs.',
  adapter: 'The calls of the hook shim. Only a caller with the token of a pane may make them.',
};

const scopeText: Record<string, string> = {
  ui: 'the UI, over the `rpc` command of the Tauri host or the WebSocket',
  cli: 'the CLI and any other client on the API socket',
  pane: 'a hook shim, with the token of a pane',
  dev: 'development and test builds only',
  phone: 'a paired phone, over the listener of the phone',
};

// ---------------------------------------------------------------------------------------------
// Types

const typeAnchor = (name: string) => name.toLowerCase();
const typeLink = (name: string) => `[\`${name}\`](/docs/reference/api/types#${typeAnchor(name)})`;
const refName = (ref: string) => ref.replace('#/$defs/', '');

function isObject(s: Schema | undefined): s is Obj {
  return typeof s === 'object' && s !== null;
}

/** A short type expression for a schema, with links to named types. */
function typeExpr(s: Schema | undefined): string {
  if (s === undefined || s === true) return 'any';
  if (s === false) return 'never';
  if (s.$ref) return typeLink(refName(s.$ref));
  if ('const' in s) return `\`${JSON.stringify(s.const)}\``;
  if (Array.isArray(s.enum))
    return s.enum.map((v: unknown) => `\`${JSON.stringify(v)}\``).join(' | ');
  const alternatives: Schema[] | undefined = s.anyOf ?? s.oneOf;
  if (alternatives && !s.properties) {
    return [...new Set(alternatives.map((a) => typeExpr(a)))].join(' | ');
  }
  if (Array.isArray(s.type)) {
    return s.type.map((t: string) => primitive({ ...s, type: t })).join(' | ');
  }
  return primitive(s);
}

function primitive(s: Obj): string {
  switch (s.type) {
    case 'array': {
      const inner = typeExpr(s.items);
      return inner.includes(' | ') ? `(${inner})[]` : `${inner}[]`;
    }
    case 'object':
      if (isObject(s.additionalProperties) || s.additionalProperties === true) {
        return `map of string to ${typeExpr(s.additionalProperties)}`;
      }
      return 'object';
    case 'integer':
      return s.format ? `integer (${s.format})` : 'integer';
    case 'number':
      return s.format ? `number (${s.format})` : 'number';
    case 'string':
      if (s.format) return `string (${s.format})`;
      if (s.contentEncoding) return `string (${s.contentEncoding})`;
      return 'string';
    case 'boolean':
      return 'boolean';
    case 'null':
      return '`null`';
    default:
      return 'any';
  }
}

/** The extra facts of a field: default, range, pattern. */
function fieldNotes(s: Schema): string {
  if (!isObject(s)) return '';
  const notes: string[] = [];
  if ('default' in s) notes.push(`Default: \`${JSON.stringify(s.default)}\`.`);
  if (typeof s.minimum === 'number' && s.minimum !== 0) notes.push(`Minimum: ${s.minimum}.`);
  if (typeof s.maximum === 'number' && !/^u?int(8|16|32|64)$/.test(String(s.format))) {
    notes.push(`Maximum: ${s.maximum}.`);
  }
  if (typeof s.pattern === 'string') notes.push(`Pattern: \`${s.pattern}\`.`);
  return notes.join(' ');
}

function describe(s: Schema | undefined): string {
  return isObject(s) ? prose(paragraphs(s.description)) : '';
}

/** A table of the properties of an object schema. */
function fieldTable(s: Obj): string {
  const props: Record<string, Schema> = s.properties ?? {};
  const required = new Set<string>(s.required ?? []);
  const names = Object.keys(props);
  if (names.length === 0) return '';
  const rows = names.map((name) => {
    const p = props[name] ?? true;
    const text = [describe(p), fieldNotes(p)].filter(Boolean).join(' ');
    return `| \`${name}\` | ${cell(typeExpr(p))} | ${required.has(name) ? 'yes' : ''} | ${cell(text)} |`;
  });
  return ['| Field | Type | Required | Description |', '|---|---|---|---|', ...rows].join('\n');
}

/** The tag of a variant of a union: a property with a `const` value. */
function variantTag(v: Schema): { field: string; value: string } | undefined {
  if (!isObject(v) || !v.properties) return undefined;
  for (const [field, p] of Object.entries<Schema>(v.properties)) {
    if (isObject(p) && 'const' in p) return { field, value: String(p.const) };
  }
  return undefined;
}

/** The body of a schema: its fields, values or variants. */
function schemaBody(s: Schema, depth: number): string {
  if (!isObject(s)) return `Any JSON value.`;
  const parts: string[] = [];
  const alternatives: Schema[] | undefined = s.oneOf ?? s.anyOf;
  if (s.properties && Object.keys(s.properties).length > 0) {
    parts.push(fieldTable(s));
  }
  if (alternatives) {
    const allConst = alternatives.every((a) => isObject(a) && 'const' in a);
    if (allConst) {
      const rows = alternatives.map((a) => {
        const v = a as Obj;
        return `| \`${JSON.stringify(v.const)}\` | ${cell(describe(v))} |`;
      });
      parts.push(['| Value | Meaning |', '|---|---|', ...rows].join('\n'));
    } else {
      const lead = s.properties ? 'Plus exactly one of these forms:' : 'One of these forms:';
      parts.push(lead);
      alternatives.forEach((a, i) => {
        const tag = variantTag(a);
        const title = tag ? `\`${tag.field}: "${tag.value}"\`` : `Form ${i + 1}`;
        const hashes = '#'.repeat(Math.min(depth + 1, 6));
        if (isObject(a) && (a.properties || a.oneOf || a.anyOf)) {
          parts.push(`${hashes} ${title}`);
          const d = describe(a);
          if (d) parts.push(d);
          parts.push(schemaBody(a, depth + 1));
        } else {
          parts.push(`- ${typeExpr(a)}${describe(a) ? ` — ${describe(a)}` : ''}`);
        }
      });
    }
  } else if (Array.isArray(s.enum)) {
    parts.push(`Values: ${s.enum.map((v: unknown) => `\`${JSON.stringify(v)}\``).join(', ')}.`);
  } else if (!s.properties || Object.keys(s.properties).length === 0) {
    const t = typeExpr({ ...s, description: undefined });
    if (t === 'object') {
      parts.push('An object with no fields: `{}`.');
    } else {
      parts.push(`Type: ${t}.${fieldNotes(s) ? ` ${fieldNotes(s)}` : ''}`);
    }
  }
  return parts.filter(Boolean).join('\n\n');
}

/** Where each named type is defined in `nh-proto`. */
function rustDefinitions(): Map<string, string> {
  const map = new Map<string, string>();
  const root = join(repoRoot, 'crates/nh-proto/src');
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (name.endsWith('.rs')) {
        const lines = readFileSync(path, 'utf8').split('\n');
        lines.forEach((line, i) => {
          const m = /^\s*pub (?:struct|enum|type) ([A-Z][A-Za-z0-9]*)/.exec(line);
          if (m?.[1] && !map.has(m[1])) map.set(m[1], `${relative(repoRoot, path)}#L${i + 1}`);
        });
      }
    }
  };
  walk(root);
  return map;
}

/** For each type, the names of the methods, projections, events and types that use it. */
function usages(contract: Contract): Map<string, Set<string>> {
  const used = new Map<string, Set<string>>();
  const add = (type: string, by: string) => {
    if (!used.has(type)) used.set(type, new Set());
    used.get(type)?.add(by);
  };
  const scan = (s: Schema | undefined, by: string) => {
    if (!isObject(s)) return;
    if (typeof s.$ref === 'string') add(refName(s.$ref), by);
    for (const value of Object.values(s)) {
      if (Array.isArray(value)) value.forEach((v) => scan(v, by));
      else if (isObject(value)) scan(value, by);
    }
  };
  for (const m of contract.methods) {
    const ns = m.name.split('.')[0] ?? '';
    const link = `[\`${m.name}\`](/docs/reference/api/methods/${ns}#${anchor(m.name)})`;
    scan(m.params, link);
    scan(m.result, link);
  }
  for (const p of contract.projections) {
    scan(p.value, `[projection \`${p.name}\`](/docs/reference/api/projections#${anchor(p.name)})`);
  }
  for (const e of contract.events) {
    scan(e.payload, `[event \`${e.kind}\`](/docs/reference/api/events#${anchor(e.kind)})`);
  }
  for (const s of contract.streams) {
    scan(s.params, `[stream \`${s.kind}\`](/docs/reference/api/streams#${anchor(s.kind)})`);
  }
  for (const [name, def] of Object.entries(contract.$defs)) scan(def, typeLink(name));
  return used;
}

// ---------------------------------------------------------------------------------------------
// Methods outside the contract

const RPC_SOURCE = 'crates/nh-core/src/debug_calls.rs';

interface ExtraMethod {
  name: string;
  params: string;
  result: string;
}

/**
 * The methods of the development builds that `crates/nh-core/src/debug_calls.rs` serves and that
 * the contract does not list (feature `debug`), from the table in its doc comment.
 */
function extraMethods(contract: Contract): ExtraMethod[] {
  const listed = new Set(contract.methods.map((m) => m.name));
  const text = readFileSync(join(repoRoot, RPC_SOURCE), 'utf8');
  const out: ExtraMethod[] = [];
  for (const line of text.split('\n')) {
    const m = /^\/\/! \| `([a-z]+\.[a-z_]+)`[^|]*\| (.*?) \| (.*?) \|$/.exec(line);
    if (!m?.[1] || listed.has(m[1])) continue;
    out.push({ name: m[1], params: m[2] ?? '', result: m[3] ?? '' });
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// The CLI

/** The `nh` commands that `gen-cli.ts` found, as `noun verb` strings. */
function cliCommands(): Set<string> {
  const path = join(import.meta.dir, 'data/cli.json');
  if (!existsSync(path)) return new Set();
  const data = JSON.parse(readFileSync(path, 'utf8')) as { path: string[] }[];
  return new Set(data.map((c) => c.path.join(' ')));
}

function cliFor(method: string, commands: Set<string>): string | undefined {
  const [noun, verb] = method.split('.');
  if (!noun || !verb) return undefined;
  for (const candidate of [`${noun} ${verb.replace(/_/g, '-')}`, `${noun} ${verb}`]) {
    if (commands.has(candidate)) return candidate;
  }
  return undefined;
}

// ---------------------------------------------------------------------------------------------
// Pages

function scopeBadges(scopes: string[]): string {
  return scopes.map((s) => `\`${s}\``).join(' ');
}

function inlineSchema(s: Schema, depth: number): string {
  if (isObject(s) && s.$ref) {
    const name = refName(s.$ref);
    return [typeLink(name), schemaBody(defs[name] ?? true, depth)].join('\n\n');
  }
  return schemaBody(s, depth);
}

let defs: Record<string, Schema> = {};

function methodPages(contract: Contract): void {
  const dir = join(OUT, 'methods');
  clearGenerated(dir);
  const commands = cliCommands();
  const byNamespace = new Map<string, Method[]>();
  for (const m of contract.methods) {
    const ns = m.name.split('.')[0] ?? '';
    byNamespace.set(ns, [...(byNamespace.get(ns) ?? []), m]);
  }
  const order = [...byNamespace.keys()];

  const indexRows = order.map((ns) => {
    const list = byNamespace.get(ns) ?? [];
    const names = list.map(
      (m) => `[\`${m.name}\`](/docs/reference/api/methods/${ns}#${anchor(m.name)})`,
    );
    return `| [\`${ns}\`](/docs/reference/api/methods/${ns}) | ${cell(namespaces[ns] ?? '')} | ${names.join(', ')} |`;
  });
  write(
    join(dir, 'index.md'),
    [
      frontMatter({
        title: 'Methods',
        description: `All ${contract.methods.length} JSON-RPC methods of the core, by namespace.`,
        sidebar_label: 'Methods',
      }),
      generatedNote(SOURCE, GENERATOR),
      `The core has ${contract.methods.length} methods in ${order.length} namespaces. A method name is \`<namespace>.<verb>\`. Each method lists its callers (scopes); a caller that is not on the list gets [\`permission_denied\`](/docs/reference/api/errors#permission-denied).`,
      '',
      '| Scope | Who | Methods |',
      '|---|---|---|',
      ...Object.keys(scopeText).map(
        (s) =>
          `| \`${s}\` | ${scopeText[s]} | ${contract.methods.filter((m) => m.scopes.includes(s)).length} |`,
      ),
      '',
      '| Namespace | What it is for | Methods |',
      '|---|---|---|',
      ...indexRows,
    ].join('\n'),
  );

  order.forEach((ns, position) => {
    const list = byNamespace.get(ns) ?? [];
    const lines: string[] = [
      frontMatter({
        title: `${ns}.*`,
        description: namespaces[ns] ?? `The ${ns} methods.`,
        sidebar_position: position + 1,
        toc_max_heading_level: 2,
      }),
      generatedNote(SOURCE, GENERATOR),
      namespaces[ns] ?? '',
      '',
      '| Method | Scopes | Params | Result |',
      '|---|---|---|---|',
      ...list.map(
        (m) =>
          `| [\`${m.name}\`](#${anchor(m.name)}) | ${scopeBadges(m.scopes)} | ${cell(typeExpr(m.params))} | ${cell(typeExpr(m.result))} |`,
      ),
      '',
    ];
    for (const m of list) {
      lines.push(`## \`${m.name}\` {#${anchor(m.name)}}`, '');
      const callers = m.scopes.map((s) => scopeText[s] ?? s).join('; ');
      lines.push(`**Scopes:** ${scopeBadges(m.scopes)} — ${callers}.`, '');
      const cli = cliFor(m.name, commands);
      if (cli) {
        const noun = cli.split(' ')[0] ?? '';
        lines.push(
          `**Command line:** [\`nh ${cli}\`](/docs/reference/cli/commands/${noun}#${anchor(cli)})`,
          '',
        );
      }
      const paramsDef =
        isObject(m.params) && m.params.$ref ? defs[refName(m.params.$ref)] : undefined;
      const paramsDescription = describe(paramsDef);
      if (paramsDescription) lines.push(paramsDescription, '');
      lines.push('### Params', '', inlineSchema(m.params, 3), '');
      lines.push('### Result', '', inlineSchema(m.result, 3), '');
    }
    const extra = extraMethods(contract).filter((e) => e.name.startsWith(`${ns}.`));
    if (extra.length) {
      lines.push(
        '## Methods outside the contract {#outside-the-contract}',
        '',
        `A build with the cargo feature \`debug\` of \`nh-core\` also serves these methods. They are not in \`nh api schema\`, so they have no generated types; the table comes from the doc comment of [\`${RPC_SOURCE}\`](${githubBlob}/${RPC_SOURCE}). The tests, \`nh-chaos\` and \`nh-golden\` use them.`,
        '',
        '| Method | Params | Result |',
        '|---|---|---|',
        ...extra.map(
          (e) => `| \`${e.name}\` | ${cell(prose(e.params))} | ${cell(prose(e.result))} |`,
        ),
        '',
      );
    }
    write(join(dir, `${ns}.md`), lines.join('\n'));
  });
}

function projectionsPage(contract: Contract): void {
  const lines: string[] = [
    frontMatter({
      title: 'Projections',
      description: `The ${contract.projections.length} read models that clients subscribe to: a snapshot, then keyed patches.`,
      sidebar_position: 3,
      toc_max_heading_level: 2,
    }),
    generatedNote(SOURCE, GENERATOR),
    'A projection is a read model that the core keeps in memory. A client calls [`projection.subscribe`](/docs/reference/api/methods/projection#projection-subscribe), gets a snapshot (`{items, order}`), then keyed patches in [`projection.message`](#notification-projection-message) notifications. A parameterized projection has a name of the form `name:<parameter>`, for example `timeline:<run>`. A singleton projection has one item with the key `_`. See [API concepts](/docs/reference/api/concepts) for versions, `since` and `resync`.',
    '',
    '| Projection | Key | Parameter | Value |',
    '|---|---|---|---|',
    ...contract.projections.map(
      (p) =>
        `| [\`${p.name}\`](#${anchor(p.name)}) | ${p.singleton ? 'singleton (`_`)' : cell(p.key)} | ${p.parameter ? cell(p.parameter) : ''} | ${cell(typeExpr(p.value))} |`,
    ),
    '',
  ];
  for (const p of contract.projections) {
    const name = p.parameter ? `${p.name}:<${p.parameter.replace(/ id$/, '')}>` : p.name;
    lines.push(`## \`${name}\` {#${anchor(p.name)}}`, '');
    lines.push(
      `**Key:** ${p.singleton ? 'singleton, the one key is `_`' : cell(p.key)}${p.parameter ? ` · **Parameter:** ${p.parameter}` : ''}`,
      '',
    );
    lines.push('### Value', '', inlineSchema(p.value, 3), '');
  }
  lines.push(...notificationSection(contract, 'projection.message'));
  write(join(OUT, 'projections.md'), lines.join('\n'));
}

function notificationSection(contract: Contract, name: string): string[] {
  const n = contract.notifications.find((x) => x.name === name);
  if (!n) return [];
  return [
    `## Notification \`${n.name}\` {#notification-${anchor(n.name)}}`,
    '',
    inlineSchema(n.params, 3),
    '',
  ];
}

function eventsPage(contract: Contract): void {
  const groups = new Map<string, typeof contract.events>();
  for (const e of contract.events) {
    const g = e.kind.split('.')[0] ?? '';
    groups.set(g, [...(groups.get(g) ?? []), e]);
  }
  const lines: string[] = [
    frontMatter({
      title: 'Events',
      description: `The ${contract.events.length} kinds of events in the event outbox.`,
      sidebar_position: 4,
    }),
    generatedNote(SOURCE, GENERATOR),
    'The core appends an event in the same SQLite transaction as the state change that it describes (the transactional outbox). A client reads the log with [`events.subscribe`](/docs/reference/api/methods/events#events-subscribe) from a cursor and gets each event in an [`events.event`](#notification-events-event) notification. When the cursor is older than the retained log, the client gets [`events.truncated`](#notification-events-truncated). `v` is the version of the payload of a kind. See [API concepts](/docs/reference/api/concepts).',
    '',
    '| Event | v | Payload |',
    '|---|---|---|',
    ...contract.events.map(
      (e) => `| [\`${e.kind}\`](#${anchor(e.kind)}) | ${e.v} | ${cell(typeExpr(e.payload))} |`,
    ),
    '',
  ];
  for (const [group, list] of groups) {
    lines.push(`## ${group} events {#group-${anchor(group)}}`, '');
    for (const e of list) {
      lines.push(`### \`${e.kind}\` {#${anchor(e.kind)}}`, '');
      lines.push(`**Version:** ${e.v}`, '');
      lines.push(inlineSchema(e.payload, 4), '');
    }
  }
  lines.push(...notificationSection(contract, 'events.event'));
  lines.push(...notificationSection(contract, 'events.truncated'));
  write(join(OUT, 'events.md'), lines.join('\n'));
}

function streamsPage(contract: Contract): void {
  const lines: string[] = [
    frontMatter({
      title: 'Streams',
      description: 'The binary streams of the contract: render, peek, scrollback and input.',
      sidebar_position: 5,
      toc_max_heading_level: 2,
    }),
    generatedNote(SOURCE, GENERATOR),
    `A stream carries binary data next to the JSON-RPC calls: \`postcard\` frames of packed cells from the core to the UI, and input from the UI to the core. The frame format has version ${contract.protocols.frame ?? '?'} and the holder protocol has version ${contract.protocols.holder ?? '?'}. See [Rendering and input](/docs/architecture/rendering-and-input) and [The desktop app](/docs/desktop/overview).`,
    '',
    '| Stream | Direction | Params |',
    '|---|---|---|',
    ...contract.streams.map(
      (s) =>
        `| [\`${s.kind}\`](#${anchor(s.kind)}) | ${s.direction.replace(/_/g, ' ')} | ${cell(typeExpr(s.params))} |`,
    ),
    '',
  ];
  for (const s of contract.streams) {
    lines.push(`## \`${s.kind}\` {#${anchor(s.kind)}}`, '');
    lines.push(`**Direction:** ${s.direction.replace(/_/g, ' ')}`, '');
    lines.push('### Params', '', inlineSchema(s.params, 3), '');
  }
  lines.push('## Notifications of the server {#notifications}', '');
  lines.push(
    'The server also sends these JSON-RPC notifications (messages without an `id`):',
    '',
    '| Notification | Params |',
    '|---|---|',
    ...contract.notifications.map((n) => `| \`${n.name}\` | ${cell(typeExpr(n.params))} |`),
    '',
  );
  lines.push(...notificationSection(contract, 'adapter.gate_result'));
  write(join(OUT, 'streams.md'), lines.join('\n'));
}

function errorsPage(contract: Contract): void {
  const errorDef = defs.ErrorCode;
  const meaning = new Map<string, string>();
  if (isObject(errorDef)) {
    for (const v of (errorDef.oneOf ?? []) as Obj[]) {
      meaning.set(String(v.const), prose(paragraphs(v.description)));
    }
  }
  const lines: string[] = [
    frontMatter({
      title: 'Errors',
      description: `The ${contract.error_codes.length} error codes of the API and their JSON-RPC numbers.`,
      sidebar_position: 6,
      toc_max_heading_level: 2,
    }),
    generatedNote(SOURCE, GENERATOR),
    describe(errorDef),
    '',
    'A protocol error has the reserved JSON-RPC number. An application error has `-32000`, and `error.data` is `{code, details}`, where `code` is one of the strings below. The UI shows `message` and acts on `code` only where the contract says so. The shape of an error as a caller sees it is [`CallError`](/docs/reference/api/types#callerror); on the wire it is [`RpcError`](/docs/reference/api/types#rpcerror).',
    '',
    '| Code | JSON-RPC | Meaning |',
    '|---|---|---|',
    ...contract.error_codes.map(
      (e) =>
        `| [\`${e.code}\`](#${anchor(e.code)}) | ${e.rpc_code} | ${cell(meaning.get(e.code) ?? '')} |`,
    ),
    '',
  ];
  for (const e of contract.error_codes) {
    lines.push(`## \`${e.code}\` {#${anchor(e.code)}}`, '');
    lines.push(`**JSON-RPC number:** \`${e.rpc_code}\``, '');
    const m = meaning.get(e.code);
    if (m) lines.push(m, '');
  }
  write(join(OUT, 'errors.md'), lines.join('\n'));
}

function typesPage(contract: Contract): void {
  const rust = rustDefinitions();
  const used = usages(contract);
  const names = Object.keys(contract.$defs).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const letters = [...new Set(names.map((n) => n[0] ?? ''))];
  const lines: string[] = [
    frontMatter({
      title: 'Types',
      description: `Every one of the ${names.length} named types of the API contract, with fields and the places that use it.`,
      sidebar_position: 7,
      toc_max_heading_level: 2,
    }),
    generatedNote(SOURCE, GENERATOR),
    `The contract names ${names.length} types. They are Rust types in \`nh-proto\` (behind its \`api\` feature) with \`serde\` and \`schemars::JsonSchema\`, and \`mise run gen:api\` turns the same schema into TypeScript in [\`packages/transport/src/generated.ts\`](${githubBlob}/packages/transport/src/generated.ts).`,
    '',
    'Jump to a letter: ' + letters.map((l) => `[${l}](#letter-${l.toLowerCase()})`).join(' · '),
    '',
  ];
  for (const letter of letters) {
    lines.push(`## ${letter} {#letter-${letter.toLowerCase()}}`, '');
    for (const name of names.filter((n) => n[0] === letter)) {
      const def = contract.$defs[name] ?? true;
      lines.push(`### \`${name}\` {#${typeAnchor(name)}}`, '');
      const d = describe(def);
      if (d) lines.push(d, '');
      lines.push(schemaBody(def, 3), '');
      const facts: string[] = [];
      const where = rust.get(name);
      if (where)
        facts.push(`**Rust:** [\`${where.replace(/#L\d+$/, '')}\`](${githubBlob}/${where})`);
      const by = used.get(name);
      if (by?.size) {
        const list = [...by].filter((b) => !b.includes(`#${typeAnchor(name)})`));
        if (list.length)
          facts.push(
            `**Used by:** ${list.slice(0, 24).join(', ')}${list.length > 24 ? ', …' : ''}`,
          );
      }
      if (facts.length) lines.push(facts.join(' · '), '');
    }
  }
  write(join(OUT, 'types.md'), lines.join('\n'));
}

function indexPage(contract: Contract): void {
  const env = contract.envelope;
  const counts = {
    methods: contract.methods.length,
    projections: contract.projections.length,
    events: contract.events.length,
    streams: contract.streams.length,
    errors: contract.error_codes.length,
    types: Object.keys(contract.$defs).length,
  };
  const lines: string[] = [
    frontMatter({
      title: 'API contract',
      description:
        'The JSON-RPC contract that the UI, the CLI and the hook shim share with the core.',
    }),
    generatedNote(SOURCE, GENERATOR),
    `The UI, the CLI and the hook shim all call the core through one contract. Its version is \`${contract.api_version}\`. The contract is declared once in Rust, in [\`crates/nh-proto/src/api\`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-proto/src/api); \`nh api schema\` prints it as JSON Schema, a snapshot test keeps it stable, and TypeScript types are generated from it. Read [API concepts](/docs/reference/api/concepts) first for the envelope, scopes, subscriptions and errors.`,
    '',
    '| Part | Count | Reference |',
    '|---|---|---|',
    `| Methods | ${counts.methods} | [Methods](/docs/reference/api/methods) |`,
    `| Projections | ${counts.projections} | [Projections](/docs/reference/api/projections) |`,
    `| Event kinds | ${counts.events} | [Events](/docs/reference/api/events) |`,
    `| Streams | ${counts.streams} | [Streams](/docs/reference/api/streams) |`,
    `| Error codes | ${counts.errors} | [Errors](/docs/reference/api/errors) |`,
    `| Named types | ${counts.types} | [Types](/docs/reference/api/types) |`,
    '',
    '## Protocol versions',
    '',
    '| Protocol | Version |',
    '|---|---|',
    `| API | \`${contract.api_version}\` |`,
    ...Object.entries(contract.protocols).map(([k, v]) => `| ${k} | ${v} |`),
    '',
    '## Envelope',
    '',
    'Every message is one JSON-RPC 2.0 object.',
    '',
  ];
  for (const [part, schema] of Object.entries(env)) {
    lines.push(`### ${part.replace(/_/g, ' ')}`, '', inlineSchema(schema, 3), '');
  }
  lines.push(
    '## Scopes',
    '',
    '| Scope | Who | Methods |',
    '|---|---|---|',
    ...Object.keys(scopeText).map(
      (s) =>
        `| \`${s}\` | ${scopeText[s]} | ${contract.methods.filter((m) => m.scopes.includes(s)).length} |`,
    ),
    '',
    '## Namespaces',
    '',
    '| Namespace | What it is for | Methods |',
    '|---|---|---|',
  );
  const seen = new Map<string, number>();
  for (const m of contract.methods) {
    const ns = m.name.split('.')[0] ?? '';
    seen.set(ns, (seen.get(ns) ?? 0) + 1);
  }
  for (const [ns, n] of seen) {
    lines.push(
      `| [\`${ns}\`](/docs/reference/api/methods/${ns}) | ${cell(namespaces[ns] ?? '')} | ${n} |`,
    );
  }
  write(join(OUT, 'index.md'), lines.join('\n'));
}

export function generateApi(): void {
  const contract = loadContract();
  defs = contract.$defs;
  indexPage(contract);
  methodPages(contract);
  projectionsPage(contract);
  eventsPage(contract);
  streamsPage(contract);
  errorsPage(contract);
  typesPage(contract);
  console.log(
    `api: ${contract.methods.length} methods, ${contract.projections.length} projections, ${contract.events.length} events, ${Object.keys(contract.$defs).length} types`,
  );
}

if (import.meta.main) generateApi();
