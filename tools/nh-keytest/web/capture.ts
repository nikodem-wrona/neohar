// The capture page: shows every keyboard and composition event of a text box with all its fields,
// and writes them as a case for `tests/keyboard/cases/`. Run it in the webview of the app for a
// real capture, and in a browser for a first look.

interface Recorded {
  type: string;
  key?: string;
  code?: string;
  location?: number;
  shift?: boolean;
  ctrl?: boolean;
  alt?: boolean;
  meta?: boolean;
  repeat?: boolean;
  is_composing?: boolean;
  key_code?: number;
  caps_lock?: boolean;
  num_lock?: boolean;
  alt_graph?: boolean;
  data?: string;
}

const must = <T extends HTMLElement>(id: string): T => {
  const element = document.getElementById(id);
  if (!element) throw new Error(`missing #${id}`);
  return element as T;
};
const box = must<HTMLTextAreaElement>('box');
const log = must<HTMLTableSectionElement>('log');
const tomlOut = must<HTMLPreElement>('toml');
const nameInput = must<HTMLInputElement>('name');
const layoutInput = must<HTMLInputElement>('layout');
const optionInput = must<HTMLSelectElement>('option');

const events: Recorded[] = [];

function quote(text: string): string {
  let out = '"';
  for (const c of text) {
    const code = c.codePointAt(0) ?? 0;
    if (c === '"') out += '\\"';
    else if (c === '\\') out += '\\\\';
    else if (code < 0x20 || code === 0x7f) out += `\\u${code.toString(16).padStart(4, '0')}`;
    else out += c;
  }
  return `${out}"`;
}

/** The event as a TOML inline table. Fields that are false, zero or empty are left out. */
function inline(e: Recorded): string {
  const fields = Object.entries(e).filter(
    ([, v]) => v !== false && v !== 0 && v !== '' && v !== undefined,
  );
  const order = ['type', 'key', 'code', 'location', 'shift', 'ctrl', 'alt', 'meta', 'repeat'];
  fields.sort(([a], [b]) => ((order.indexOf(a) + 100) % 100) - ((order.indexOf(b) + 100) % 100));
  const text = fields.map(([k, v]) => `${k} = ${typeof v === 'string' ? quote(v) : String(v)}`);
  return `{ ${text.join(', ')} }`;
}

function platform(): string {
  const p = navigator.platform.toLowerCase();
  if (p.includes('mac')) return 'macos';
  return p.includes('linux') ? 'linux' : 'any';
}

function render(): void {
  const lines = [
    '[[case]]',
    `name = ${quote(nameInput.value || 'capture')}`,
    `platform = ${quote(platform())}`,
    `config = { macos_option_as_alt = ${quote(optionInput.value)} }`,
    'events = [',
    ...events.map((e) => `  ${inline(e)},`),
    ']',
    'expect = ""',
    `source = ${quote(`real capture: ${layoutInput.value || 'layout?'}, ${navigator.userAgent}`)}`,
  ];
  tomlOut.textContent = lines.join('\n');
}

function row(e: Recorded, states: string): void {
  const tr = document.createElement('tr');
  const mods = [e.shift && 'shift', e.ctrl && 'ctrl', e.alt && 'alt', e.meta && 'meta']
    .filter(Boolean)
    .join('+');
  for (const cell of [
    e.type,
    e.key ?? '',
    e.code ?? '',
    String(e.location ?? ''),
    mods,
    String(e.repeat ?? ''),
    String(e.is_composing ?? ''),
    String(e.key_code ?? ''),
    states,
    e.data ?? '',
  ]) {
    const td = document.createElement('td');
    td.textContent = cell;
    tr.append(td);
  }
  log.prepend(tr);
}

function onKey(event: KeyboardEvent): void {
  const states = ['CapsLock', 'NumLock', 'AltGraph'].filter((s) => event.getModifierState(s));
  const e: Recorded = {
    type: event.type,
    key: event.key,
    code: event.code,
    location: event.location,
    shift: event.shiftKey,
    ctrl: event.ctrlKey,
    alt: event.altKey,
    meta: event.metaKey,
    repeat: event.repeat,
    is_composing: event.isComposing,
    key_code: event.keyCode,
    caps_lock: states.includes('CapsLock'),
    num_lock: states.includes('NumLock'),
    alt_graph: states.includes('AltGraph'),
  };
  events.push(e);
  row(e, states.join(' '));
  render();
}

function onComposition(event: CompositionEvent): void {
  const e: Recorded = { type: event.type, data: event.data };
  events.push(e);
  row(e, '');
  render();
}

box.addEventListener('keydown', onKey);
box.addEventListener('keyup', onKey);
for (const type of ['compositionstart', 'compositionupdate', 'compositionend']) {
  box.addEventListener(type, (e) => onComposition(e as CompositionEvent));
}
for (const input of [nameInput, layoutInput, optionInput]) input.addEventListener('input', render);
must('clear').addEventListener('click', () => {
  events.length = 0;
  log.replaceChildren();
  box.value = '';
  render();
  box.focus();
});
must('copy').addEventListener('click', () => {
  void navigator.clipboard.writeText(tomlOut.textContent ?? '');
});
render();

export {};
