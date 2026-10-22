// Writes the generated files of the keyboard fidelity matrix: `tests/keyboard/cases/gen-*.toml`.
//
//   bun tests/keyboard/generate.ts          write the files
//   bun test tests/keyboard                 check that the files on disk are what this script writes
//
// The expected bytes are written here from the documents named in each `source`: the xterm
// control sequences (`ctlseqs`), the kitty keyboard protocol and the ncurses terminfo entry of
// `xterm-256color` (checked against `infocmp` in `generate.test.ts`). They are computed with
// formulas of those documents, in TypeScript, apart from the Rust encoder that the cases test.
// The kitty cases and the modifyOtherKeys cases are written out by hand, byte by byte.
//
// Cases that need a person to think (dead keys, AltGr, input methods, the Option key, the mouse,
// paste and focus) are in the hand written files of the same directory.

import { join } from 'node:path';
import { table, type Value } from './toml';

type Mods = { shift?: boolean; ctrl?: boolean; alt?: boolean; meta?: boolean };
type Locks = { caps?: boolean; num?: boolean };
type Kind = 'press' | 'repeat' | 'release';
type Row = Record<string, Value | undefined>;

export interface Spec {
  /** `KeyboardEvent.code`. */
  code: string;
  /** `KeyboardEvent.key` without Shift. */
  key: string;
  /** `KeyboardEvent.key` with Shift, when it differs. */
  shifted?: string;
  /** `KeyboardEvent.location`. */
  location?: number;
}

const ESC = '\x1b';
const CSI = `${ESC}[`;
const SS3 = `${ESC}O`;

/** Bytes as lowercase hexadecimal. A string is UTF-8, a number is one byte. */
export function hex(...parts: (string | number)[]): string {
  const bytes = parts.flatMap((p) =>
    typeof p === 'number' ? [p] : [...new TextEncoder().encode(p)],
  );
  return bytes.map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** The modifier parameter of xterm and kitty: 1 + Shift 1 + Alt 2 + Ctrl 4 + Meta 8. */
export const param = (m: Mods): number =>
  1 + (m.shift ? 1 : 0) + (m.alt ? 2 : 0) + (m.ctrl ? 4 : 0) + (m.meta ? 8 : 0);

const isPlain = (m: Mods): boolean => param(m) === 1;

export function label(m: Mods, locks: Locks = {}): string {
  const names = [
    m.ctrl && 'ctrl',
    m.alt && 'alt',
    m.shift && 'shift',
    m.meta && 'meta',
    locks.caps && 'capslock',
    locks.num && 'numlock',
  ].filter(Boolean);
  return names.length ? names.join('+') : 'plain';
}

const modNames = (m: Mods, l: Locks = {}): string[] =>
  [
    m.shift && 'shift',
    m.ctrl && 'ctrl',
    m.alt && 'alt',
    m.meta && 'meta',
    l.caps && 'caps_lock',
    l.num && 'num_lock',
  ].filter((x): x is string => typeof x === 'string');

interface Chord {
  spec: Spec;
  mods: Mods;
  locks?: Locks;
  kind?: Kind;
}

/** The `key` of the event: Shift picks the shifted character, and Caps Lock does for letters. */
function eventKey({ spec, mods, locks = {} }: Chord): string {
  const letter = /^[a-z]$/.test(spec.key);
  const upper = letter ? !!mods.shift !== !!locks.caps : !!mods.shift;
  return upper && spec.shifted !== undefined ? spec.shifted : spec.key;
}

function domEvent(chord: Chord): Row {
  const { spec, mods, locks = {}, kind = 'press' } = chord;
  const key = eventKey(chord);
  return {
    type: kind === 'release' ? 'keyup' : 'keydown',
    key,
    code: spec.code,
    location: spec.location || undefined,
    shift: mods.shift || undefined,
    ctrl: mods.ctrl || undefined,
    alt: mods.alt || undefined,
    meta: mods.meta || undefined,
    repeat: kind === 'repeat' || undefined,
    caps_lock: locks.caps || undefined,
    num_lock: locks.num || undefined,
  };
}

/** What the keyboard layer must produce for the chord (`packages/ui/src/input/keyboard.ts`). */
function logicalEvent(chord: Chord): Row {
  const { spec, mods, locks = {}, kind = 'press' } = chord;
  const key = eventKey(chord);
  const printable = [...key].length === 1;
  return {
    type: 'key',
    code: spec.code,
    key,
    mods: modNames(mods, locks),
    kind,
    text: printable && !mods.ctrl && !mods.meta && kind !== 'release' ? key : undefined,
    location: spec.location || undefined,
  };
}

export interface CaseOptions {
  name: string;
  chords: Chord[];
  expect: string;
  source: string;
  modes?: string[];
  kitty?: number;
  mok?: number;
  config?: Record<string, string> | undefined;
  platform?: string;
}

export function keyCase(o: CaseOptions): Row {
  return {
    name: o.name,
    platform: o.platform && o.platform !== 'any' ? o.platform : undefined,
    modes: o.modes?.length ? o.modes : undefined,
    kitty: o.kitty || undefined,
    modify_other_keys: o.mok || undefined,
    config: o.config,
    events: o.chords.map(domEvent),
    logical: o.chords.map(logicalEvent),
    expect: o.expect,
    source: o.source,
  };
}

// ---------------------------------------------------------------------------------------------
// The US layout.

const LETTERS: Spec[] = [...'abcdefghijklmnopqrstuvwxyz'].map((c) => ({
  code: `Key${c.toUpperCase()}`,
  key: c,
  shifted: c.toUpperCase(),
}));
const DIGIT_SPECS: Spec[] = [...Array(10).keys()].map((n) => ({
  code: `Digit${n}`,
  key: String(n),
  shifted: ')!@#$%^&*('[n] as string,
}));
const PUNCTUATION: Spec[] = [
  { code: 'Backquote', key: '`', shifted: '~' },
  { code: 'Minus', key: '-', shifted: '_' },
  { code: 'Equal', key: '=', shifted: '+' },
  { code: 'BracketLeft', key: '[', shifted: '{' },
  { code: 'BracketRight', key: ']', shifted: '}' },
  { code: 'Backslash', key: '\\', shifted: '|' },
  { code: 'Semicolon', key: ';', shifted: ':' },
  { code: 'Quote', key: "'", shifted: '"' },
  { code: 'Comma', key: ',', shifted: '<' },
  { code: 'Period', key: '.', shifted: '>' },
  { code: 'Slash', key: '/', shifted: '?' },
];

const NONE: Mods = {};
const SHIFT: Mods = { shift: true };
const CTRL: Mods = { ctrl: true };
const ALT: Mods = { alt: true };
const META: Mods = { meta: true };
const CTRL_SHIFT: Mods = { ctrl: true, shift: true };
const CTRL_ALT: Mods = { ctrl: true, alt: true };
const SHIFT_ALT: Mods = { shift: true, alt: true };
const ALL: Mods = { shift: true, alt: true, ctrl: true };

/** The six chords of the text matrix. */
const TEXT_MODS = [NONE, SHIFT, CTRL, ALT, CTRL_SHIFT, CTRL_ALT];
/** The nine chords of the navigation and function keys. */
const NAV_MODS = [NONE, SHIFT, ALT, SHIFT_ALT, CTRL, META, CTRL_SHIFT, CTRL_ALT, ALL];

// ---------------------------------------------------------------------------------------------
// Legacy text keys.

const controlByte = (c: string): number | undefined => {
  if (/^[a-zA-Z]$/.test(c)) return c.charCodeAt(0) & 0x1f;
  return { ' ': 0, '@': 0, '[': 0x1b, '\\': 0x1c, ']': 0x1d, '^': 0x1e, _: 0x1f }[c];
};

const TEXT_SOURCE = 'xterm ctlseqs: Control Characters, Alt and Meta Keys';

function legacyText(specs: Spec[], group: string): Row[] {
  const rows: Row[] = [];
  for (const spec of specs) {
    for (const mods of TEXT_MODS) {
      const c = mods.shift ? (spec.shifted as string) : spec.key;
      const control = mods.ctrl ? controlByte(c) : undefined;
      const body = control === undefined ? c : String.fromCharCode(control);
      rows.push(
        keyCase({
          name: `${group} ${spec.code} ${label(mods)}, legacy`,
          chords: [{ spec, mods }],
          expect: hex(mods.alt ? ESC : '', ...(control === undefined ? [body] : [control])),
          source: TEXT_SOURCE,
        }),
      );
    }
  }
  return rows;
}

// ---------------------------------------------------------------------------------------------
// Navigation and function keys.

const NAV_SOURCE = 'xterm ctlseqs: PC-Style Function Keys, DECCKM; terminfo xterm-256color';

const NAV_LETTER: [string, string][] = [
  ['ArrowUp', 'A'],
  ['ArrowDown', 'B'],
  ['ArrowRight', 'C'],
  ['ArrowLeft', 'D'],
  ['Home', 'H'],
  ['End', 'F'],
];
const NAV_TILDE: [string, number][] = [
  ['Insert', 2],
  ['Delete', 3],
  ['PageUp', 5],
  ['PageDown', 6],
];

function navigation(): Row[] {
  const rows: Row[] = [];
  for (const app of [false, true]) {
    for (const mods of NAV_MODS) {
      const tag = `${label(mods)}${app ? ', application cursor' : ''}`;
      for (const [key, letter] of NAV_LETTER) {
        const expect = isPlain(mods)
          ? `${app ? SS3 : CSI}${letter}`
          : `${CSI}1;${param(mods)}${letter}`;
        rows.push(
          keyCase({
            name: `${key} ${tag}`,
            chords: [{ spec: { code: key, key }, mods }],
            modes: app ? ['app_cursor'] : [],
            expect: hex(expect),
            source: NAV_SOURCE,
          }),
        );
      }
      for (const [key, n] of NAV_TILDE) {
        const expect = isPlain(mods) ? `${CSI}${n}~` : `${CSI}${n};${param(mods)}~`;
        rows.push(
          keyCase({
            name: `${key} ${tag}`,
            chords: [{ spec: { code: key, key }, mods }],
            modes: app ? ['app_cursor'] : [],
            expect: hex(expect),
            source: NAV_SOURCE,
          }),
        );
      }
    }
  }
  return rows;
}

const FUNCTION_TILDE: Record<number, number> = {
  5: 15,
  6: 17,
  7: 18,
  8: 19,
  9: 20,
  10: 21,
  11: 23,
  12: 24,
  13: 25,
  14: 26,
  15: 28,
  16: 29,
  17: 31,
  18: 32,
  19: 33,
  20: 34,
};

function functionKeys(): Row[] {
  const rows: Row[] = [];
  const source = 'xterm ctlseqs: PC-Style Function Keys, VT220 F13 to F20; terminfo xterm-256color';
  for (let n = 1; n <= 20; n++) {
    const mods = n <= 12 ? NAV_MODS : [NONE, SHIFT, CTRL];
    for (const m of mods) {
      let expect: string;
      if (n <= 4) {
        const letter = 'PQRS'[n - 1] as string;
        expect = isPlain(m) ? `${SS3}${letter}` : `${CSI}1;${param(m)}${letter}`;
      } else {
        const code = FUNCTION_TILDE[n] as number;
        expect = isPlain(m) ? `${CSI}${code}~` : `${CSI}${code};${param(m)}~`;
      }
      rows.push(
        keyCase({
          name: `F${n} ${label(m)}`,
          chords: [{ spec: { code: `F${n}`, key: `F${n}` }, mods: m }],
          expect: hex(expect),
          source,
        }),
      );
    }
  }
  return rows;
}

// ---------------------------------------------------------------------------------------------
// Enter, Tab, Backspace, Escape, Space and the keypad.

function specialKeys(): Row[] {
  const rows: Row[] = [];
  const source = 'xterm ctlseqs: Control Characters, Alt and Meta Keys';
  const keys: { spec: Spec; bytes: (m: Mods, bs: boolean) => string | number[] }[] = [
    { spec: { code: 'Enter', key: 'Enter' }, bytes: () => '\r' },
    { spec: { code: 'Tab', key: 'Tab' }, bytes: (m) => (m.shift ? `${CSI}Z` : '\t') },
    {
      spec: { code: 'Backspace', key: 'Backspace' },
      bytes: (m, bs) => [(m.ctrl ? !bs : bs) ? 0x08 : 0x7f],
    },
    { spec: { code: 'Escape', key: 'Escape' }, bytes: () => '\x1b' },
    {
      spec: { code: 'Space', key: ' ' },
      bytes: (m) => (m.ctrl ? [0x00] : ' '),
    },
  ];
  for (const { spec, bytes } of keys) {
    const backspaceModes = spec.code === 'Backspace' ? [false, true] : [false];
    for (const bs of backspaceModes) {
      for (const mods of TEXT_MODS) {
        const body = bytes(mods, bs);
        rows.push(
          keyCase({
            name: `${spec.code} ${label(mods)}${bs ? ', backspace sends BS' : ''}`,
            chords: [{ spec, mods }],
            config: bs ? { backspace: 'bs' } : undefined,
            expect: hex(mods.alt ? ESC : '', ...(typeof body === 'string' ? [body] : body)),
            source,
          }),
        );
      }
    }
  }
  rows.push(
    keyCase({
      name: 'Enter in new line mode',
      chords: [{ spec: { code: 'Enter', key: 'Enter' }, mods: NONE }],
      modes: ['lnm'],
      expect: hex('\r\n'),
      source: 'xterm ctlseqs: LNM (mode 20), Return sends CR LF',
    }),
  );
  return rows;
}

const KEYPAD: { code: string; key: string; app: string }[] = [
  ...[...Array(10).keys()].map((n) => ({
    code: `Numpad${n}`,
    key: String(n),
    app: 'pqrstuvwxy'[n] as string,
  })),
  { code: 'NumpadDecimal', key: '.', app: 'n' },
  { code: 'NumpadDivide', key: '/', app: 'o' },
  { code: 'NumpadMultiply', key: '*', app: 'j' },
  { code: 'NumpadSubtract', key: '-', app: 'm' },
  { code: 'NumpadAdd', key: '+', app: 'k' },
  { code: 'NumpadEnter', key: 'Enter', app: 'M' },
  { code: 'NumpadEqual', key: '=', app: 'X' },
];

// The keys of the keypad when Num Lock is off.
const KEYPAD_NAV: { code: string; key: string; letter?: string; tilde?: number }[] = [
  { code: 'Numpad0', key: 'Insert', tilde: 2 },
  { code: 'Numpad1', key: 'End', letter: 'F' },
  { code: 'Numpad2', key: 'ArrowDown', letter: 'B' },
  { code: 'Numpad3', key: 'PageDown', tilde: 6 },
  { code: 'Numpad4', key: 'ArrowLeft', letter: 'D' },
  { code: 'Numpad5', key: 'Clear', letter: 'E' },
  { code: 'Numpad6', key: 'ArrowRight', letter: 'C' },
  { code: 'Numpad7', key: 'Home', letter: 'H' },
  { code: 'Numpad8', key: 'ArrowUp', letter: 'A' },
  { code: 'Numpad9', key: 'PageUp', tilde: 5 },
  { code: 'NumpadDecimal', key: 'Delete', tilde: 3 },
];

function keypad(): Row[] {
  const rows: Row[] = [];
  for (const app of [false, true]) {
    for (const k of KEYPAD) {
      const spec: Spec = { code: k.code, key: k.key, location: 3 };
      const plain = k.key === 'Enter' ? '\r' : k.key;
      rows.push(
        keyCase({
          name: `${k.code} ${app ? 'application keypad' : 'numeric keypad'}`,
          chords: [{ spec, mods: NONE, locks: { num: true } }],
          modes: app ? ['app_keypad'] : [],
          expect: hex(app ? `${SS3}${k.app}` : plain),
          source: app
            ? 'xterm ctlseqs: VT100 Application Keypad (DECKPAM); terminfo xterm-256color kent'
            : 'xterm ctlseqs: the keypad sends its characters outside application keypad mode',
        }),
      );
    }
  }
  for (const k of KEYPAD_NAV) {
    for (const app of [false, true]) {
      const expect = k.letter !== undefined ? `${CSI}${k.letter}` : `${CSI}${k.tilde}~`;
      rows.push(
        keyCase({
          name: `${k.code} with Num Lock off is ${k.key}${app ? ', application keypad' : ''}`,
          chords: [{ spec: { code: k.code, key: k.key, location: 3 }, mods: NONE }],
          modes: app ? ['app_keypad'] : [],
          expect: hex(expect),
          source: 'xterm ctlseqs: with Num Lock off the keypad keys are cursor and editing keys',
        }),
      );
    }
  }
  for (const k of [KEYPAD[15], KEYPAD[14], KEYPAD[12]] as (typeof KEYPAD)[number][]) {
    rows.push(
      keyCase({
        name: `${k.code} shift, application keypad`,
        chords: [{ spec: { code: k.code, key: k.key, location: 3 }, mods: SHIFT }],
        modes: ['app_keypad'],
        expect: hex(`${SS3}2${k.app}`),
        source: 'xterm ctlseqs: a modified application keypad key is SS3 m final',
      }),
    );
  }
  return rows;
}

// ---------------------------------------------------------------------------------------------
// The kitty keyboard protocol, written out.

const KITTY_SOURCE = 'kitty keyboard protocol';
const K = (name: string, kitty: number, chords: Chord[], expect: string, modes: string[] = []) =>
  keyCase({ name: `kitty ${kitty}: ${name}`, kitty, chords, expect, source: KITTY_SOURCE, modes });
const key = (code: string, k: string, shifted?: string, location?: number): Spec => ({
  code,
  key: k,
  ...(shifted === undefined ? {} : { shifted }),
  ...(location === undefined ? {} : { location }),
});
const A = key('KeyA', 'a', 'A');
const C = key('KeyC', 'c', 'C');
const ENTER = key('Enter', 'Enter');
const TAB = key('Tab', 'Tab');
const BACKSPACE = key('Backspace', 'Backspace');
const ESCAPE = key('Escape', 'Escape');
const UP = key('ArrowUp', 'ArrowUp');
const F1 = key('F1', 'F1');
const F3 = key('F3', 'F3');
const F5 = key('F5', 'F5');
const DELETE = key('Delete', 'Delete');
const SPACE = key('Space', ' ');
const press = (spec: Spec, mods: Mods = NONE, locks: Locks = {}): Chord => ({ spec, mods, locks });
const repeat = (spec: Spec, mods: Mods = NONE): Chord => ({ spec, mods, kind: 'repeat' });
const release = (spec: Spec, mods: Mods = NONE): Chord => ({ spec, mods, kind: 'release' });
const u = (code: number, mods?: string): string => `${CSI}${code}${mods ? `;${mods}` : ''}u`;

function kitty(): Row[] {
  const rows: Row[] = [];
  const add = (r: Row) => rows.push(r);
  // Flag 1: disambiguate.
  add(K('a', 1, [press(A)], hex('a')));
  add(K('shift+a is text', 1, [press(A, SHIFT)], hex('A')));
  add(K('ctrl+c', 1, [press(C, CTRL)], hex(u(99, '5'))));
  add(K('alt+a', 1, [press(A, ALT)], hex(u(97, '3'))));
  add(K('ctrl+alt+a', 1, [press(A, CTRL_ALT)], hex(u(97, '7'))));
  add(K('ctrl+shift+a', 1, [press(A, CTRL_SHIFT)], hex(u(97, '6'))));
  add(K('shift+alt+a', 1, [press(A, SHIFT_ALT)], hex(u(97, '4'))));
  add(K('meta+a', 1, [press(A, META)], hex(u(97, '9'))));
  add(K('ctrl+space', 1, [press(SPACE, CTRL)], hex(u(32, '5'))));
  add(K('shift+space is text', 1, [press(SPACE, SHIFT)], hex(' ')));
  add(K('escape', 1, [press(ESCAPE)], hex(u(27))));
  add(K('alt+escape', 1, [press(ESCAPE, ALT)], hex(u(27, '3'))));
  add(K('enter keeps its byte', 1, [press(ENTER)], hex('\r')));
  add(K('shift+enter', 1, [press(ENTER, SHIFT)], hex(u(13, '2'))));
  add(K('ctrl+enter', 1, [press(ENTER, CTRL)], hex(u(13, '5'))));
  add(K('tab keeps its byte', 1, [press(TAB)], hex('\t')));
  add(K('shift+tab', 1, [press(TAB, SHIFT)], hex(u(9, '2'))));
  add(K('backspace keeps its byte', 1, [press(BACKSPACE)], hex(0x7f)));
  add(K('ctrl+backspace', 1, [press(BACKSPACE, CTRL)], hex(u(127, '5'))));
  add(K('up', 1, [press(UP)], hex(`${CSI}A`)));
  add(K('up, application cursor', 1, [press(UP)], hex(`${SS3}A`), ['app_cursor']));
  add(K('ctrl+up', 1, [press(UP, CTRL)], hex(`${CSI}1;5A`)));
  add(K('f1', 1, [press(F1)], hex(`${SS3}P`)));
  add(K('shift+f1', 1, [press(F1, SHIFT)], hex(`${CSI}1;2P`)));
  add(K('f3', 1, [press(F3)], hex(`${SS3}R`)));
  add(K('ctrl+f3 is CSI 13 ~', 1, [press(F3, CTRL)], hex(`${CSI}13;5~`)));
  add(K('f5', 1, [press(F5)], hex(`${CSI}15~`)));
  add(K('alt+f5', 1, [press(F5, ALT)], hex(`${CSI}15;3~`)));
  add(K('delete', 1, [press(DELETE)], hex(`${CSI}3~`)));
  add(K('shift+delete', 1, [press(DELETE, SHIFT)], hex(`${CSI}3;2~`)));
  add(K('f13', 1, [press(key('F13', 'F13'))], hex(u(57376))));
  add(K('f35 with ctrl', 1, [press(key('F35', 'F35'), CTRL)], hex(u(57398, '5'))));
  add(K('menu', 1, [press(key('ContextMenu', 'ContextMenu'))], hex(u(57363))));
  add(K('print screen', 1, [press(key('PrintScreen', 'PrintScreen'))], hex(u(57361))));
  add(K('keypad enter', 1, [press(key('NumpadEnter', 'Enter', undefined, 3))], hex(u(57414))));
  add(K('keypad 1 is text', 1, [press(key('Numpad1', '1', undefined, 3))], hex('1')));
  add(
    K('keypad 1 with Num Lock off', 1, [press(key('Numpad1', 'End', undefined, 3))], hex(u(57424))),
  );
  add(K('keypad ctrl+5', 1, [press(key('Numpad5', '5', undefined, 3), CTRL)], hex(u(57404, '5'))));
  add(K('caps lock gives upper case text', 1, [press(A, NONE, { caps: true })], hex('A')));
  add(K('lock bits stay out', 1, [press(C, CTRL, { caps: true, num: true })], hex(u(99, '5'))));
  add(K('shift is dropped: shift+1 is text', 1, [press(key('Digit1', '1', '!'), SHIFT)], hex('!')));
  add(
    K(
      'ctrl+shift+1 uses the key without shift',
      1,
      [press(key('Digit1', '1', '!'), CTRL_SHIFT)],
      hex(u(49, '6')),
    ),
  );
  // Flags 1 and 2: event types.
  const F2 = 3;
  add(K('a press, repeat and release', F2, [press(A), repeat(A), release(A)], hex('aa')));
  add(
    K(
      'ctrl+c press, repeat, release',
      F2,
      [press(C, CTRL), repeat(C, CTRL), release(C, CTRL)],
      hex(u(99, '5'), u(99, '5:2'), u(99, '5:3')),
    ),
  );
  add(K('escape release', F2, [press(ESCAPE), release(ESCAPE)], hex(u(27), u(27, '1:3'))));
  add(K('enter has no release', F2, [press(ENTER), release(ENTER)], hex('\r')));
  add(K('enter repeat is a byte', F2, [repeat(ENTER)], hex('\r')));
  add(K('up press and release', F2, [press(UP), release(UP)], hex(`${CSI}A`, `${CSI}1;1:3A`)));
  add(K('up repeat', F2, [repeat(UP)], hex(`${CSI}1;1:2A`)));
  add(K('ctrl+up release', F2, [release(UP, CTRL)], hex(`${CSI}1;5:3A`)));
  add(K('f5 release', F2, [release(F5)], hex(`${CSI}15;1:3~`)));
  add(K('f3 release', F2, [release(F3)], hex(`${CSI}13;1:3~`)));
  add(K('delete release', F2, [release(DELETE)], hex(`${CSI}3;1:3~`)));
  add(K('shift+enter release', F2, [release(ENTER, SHIFT)], hex(u(13, '2:3'))));
  add(K('a release alone', F2, [release(A)], ''));
  add(K('repeat without the flag is a press', 1, [repeat(C, CTRL)], hex(u(99, '5'))));
  add(K('release without the flag is nothing', 1, [release(C, CTRL)], ''));
  // Flag 4: alternate keys. They go into events that are CSI u.
  add(K('ctrl+shift+a has the shifted key', 4 | 1, [press(A, CTRL_SHIFT)], hex(`${CSI}97:65;6u`)));
  add(K('shift+a is still text', 4 | 1, [press(A, SHIFT)], hex('A')));
  add(K('ctrl+c has no alternates', 4 | 1, [press(C, CTRL)], hex(u(99, '5'))));
  add(
    K(
      'base layout key of a Cyrillic layout',
      4 | 1,
      [press(key('KeyC', 'с'), CTRL)],
      hex(`${CSI}1089::99;5u`),
    ),
  );
  add(
    K(
      'alt+ß on a German layout has the base layout key',
      4 | 1,
      [press(key('Minus', 'ß'), ALT)],
      hex(`${CSI}223::45;3u`),
    ),
  );
  add(
    K(
      'ctrl+shift+1 has the shifted key',
      4 | 1,
      [press(key('Digit1', '1', '!'), CTRL_SHIFT)],
      hex(`${CSI}49:33;6u`),
    ),
  );
  // Flag 8: every key.
  const ALLKEYS = 8 | 1;
  add(K('a', ALLKEYS, [press(A)], hex(u(97))));
  add(K('shift+a', ALLKEYS, [press(A, SHIFT)], hex(u(97, '2'))));
  add(K('caps lock and a', ALLKEYS, [press(A, NONE, { caps: true })], hex(u(97, '65'))));
  add(K('num lock and a', ALLKEYS, [press(A, NONE, { num: true })], hex(u(97, '129'))));
  add(K('enter', ALLKEYS, [press(ENTER)], hex(u(13))));
  add(K('tab', ALLKEYS, [press(TAB)], hex(u(9))));
  add(K('backspace', ALLKEYS, [press(BACKSPACE)], hex(u(127))));
  add(K('up', ALLKEYS, [press(UP)], hex(`${CSI}A`)));
  add(K('num lock and up', ALLKEYS, [press(UP, NONE, { num: true })], hex(`${CSI}1;129A`)));
  add(
    K(
      'left shift',
      ALLKEYS,
      [press(key('ShiftLeft', 'Shift', undefined, 1), SHIFT)],
      hex(u(57441, '2')),
    ),
  );
  add(
    K(
      'right shift release',
      ALLKEYS | 2,
      [release(key('ShiftRight', 'Shift', undefined, 2))],
      hex(u(57447, '1:3')),
    ),
  );
  add(
    K(
      'left control',
      ALLKEYS,
      [press(key('ControlLeft', 'Control', undefined, 1), CTRL)],
      hex(u(57442, '5')),
    ),
  );
  add(
    K('left alt', ALLKEYS, [press(key('AltLeft', 'Alt', undefined, 1), ALT)], hex(u(57443, '3'))),
  );
  add(
    K(
      'left meta is super',
      ALLKEYS,
      [press(key('MetaLeft', 'Meta', undefined, 1), META)],
      hex(u(57444, '9')),
    ),
  );
  add(
    K(
      'caps lock key',
      ALLKEYS,
      [press(key('CapsLock', 'CapsLock'), NONE, { caps: true })],
      hex(u(57358, '65')),
    ),
  );
  add(
    K(
      'modifier keys are silent without flag 8',
      1,
      [press(key('ShiftLeft', 'Shift', undefined, 1), SHIFT)],
      '',
    ),
  );
  add(K('keypad 1', ALLKEYS, [press(key('Numpad1', '1', undefined, 3))], hex(u(57400))));
  add(
    K('keypad enter', ALLKEYS, [press(key('NumpadEnter', 'Enter', undefined, 3))], hex(u(57414))),
  );
  add(K('a press and release', ALLKEYS | 2, [press(A), release(A)], hex(u(97), u(97, '1:3'))));
  add(K('a repeat', ALLKEYS | 2, [repeat(A)], hex(u(97, '1:2'))));
  add(K('enter release', ALLKEYS | 2, [release(ENTER)], hex(u(13, '1:3'))));
  // Flag 16: associated text.
  const FULL = 1 | 2 | 4 | 8 | 16;
  add(K('a', FULL, [press(A)], hex(`${CSI}97;;97u`)));
  add(K('shift+a', FULL, [press(A, SHIFT)], hex(`${CSI}97:65;2;65u`)));
  add(K('ctrl+a has no text', FULL, [press(A, CTRL)], hex(u(97, '5'))));
  add(K('alt+a has no text', FULL, [press(A, ALT)], hex(u(97, '3'))));
  add(K('caps lock and a', FULL, [press(A, NONE, { caps: true })], hex(`${CSI}97;65;65u`)));
  add(K('a release has no text', FULL, [release(A)], hex(u(97, '1:3'))));
  add(K('a repeat', FULL, [repeat(A)], hex(`${CSI}97;1:2;97u`)));
  add(K('enter has no text', FULL, [press(ENTER)], hex(u(13))));
  add(K('keypad 1', FULL, [press(key('Numpad1', '1', undefined, 3))], hex(`${CSI}57400;;49u`)));
  add(K('é', FULL, [press(key('Quote', 'é'))], hex(`${CSI}233::39;;233u`)));
  return rows;
}

// ---------------------------------------------------------------------------------------------
// modifyOtherKeys.

const MOK_SOURCE = 'xterm ctlseqs: modifyOtherKeys (CSI 27 ; m ; code ~)';

function modifyOtherKeys(): Row[] {
  const rows: Row[] = [];
  const M = (name: string, level: number, chords: Chord[], expect: string) =>
    rows.push(
      keyCase({
        name: `modifyOtherKeys ${level}: ${name}`,
        mok: level,
        chords,
        expect,
        source: MOK_SOURCE,
      }),
    );
  const mok = (code: number, m: number) => hex(`${CSI}27;${m};${code}~`);
  const I = key('KeyI', 'i', 'I');
  const ONE = key('Digit1', '1', '!');
  const SEMI = key('Semicolon', ';', ':');
  // Level 2.
  M('ctrl+i', 2, [press(I, CTRL)], mok(105, 5));
  M('ctrl+a', 2, [press(A, CTRL)], mok(97, 5));
  M('alt+a', 2, [press(A, ALT)], mok(97, 3));
  M('meta+a', 2, [press(A, META)], mok(97, 9));
  M('ctrl+shift+a', 2, [press(A, CTRL_SHIFT)], mok(65, 6));
  M('ctrl+alt+a', 2, [press(A, CTRL_ALT)], mok(97, 7));
  M('shift+a is text', 2, [press(A, SHIFT)], hex('A'));
  M('plain a', 2, [press(A)], hex('a'));
  M('ctrl+1', 2, [press(ONE, CTRL)], mok(49, 5));
  M('ctrl+;', 2, [press(SEMI, CTRL)], mok(59, 5));
  M('ctrl+shift+;', 2, [press(SEMI, CTRL_SHIFT)], mok(58, 6));
  M('ctrl+space', 2, [press(SPACE, CTRL)], mok(32, 5));
  M('shift+space is text', 2, [press(SPACE, SHIFT)], hex(' '));
  M('shift+enter', 2, [press(ENTER, SHIFT)], mok(13, 2));
  M('ctrl+enter', 2, [press(ENTER, CTRL)], mok(13, 5));
  M('plain enter', 2, [press(ENTER)], hex('\r'));
  M('shift+tab', 2, [press(TAB, SHIFT)], mok(9, 2));
  M('ctrl+tab', 2, [press(TAB, CTRL)], mok(9, 5));
  M('ctrl+backspace', 2, [press(BACKSPACE, CTRL)], mok(127, 5));
  M('shift+escape', 2, [press(ESCAPE, SHIFT)], mok(27, 2));
  M('alt+escape', 2, [press(ESCAPE, ALT)], mok(27, 3));
  M('ctrl+up is not changed', 2, [press(UP, CTRL)], hex(`${CSI}1;5A`));
  M('ctrl+f5 is not changed', 2, [press(F5, CTRL)], hex(`${CSI}15;5~`));
  M('alt+é', 2, [press(key('Quote', 'é'), ALT)], mok(233, 3));
  // Level 1: only where the legacy form loses a modifier.
  M('ctrl+i is still tab', 1, [press(I, CTRL)], hex(0x09));
  M('ctrl+a is still C0', 1, [press(A, CTRL)], hex(0x01));
  M('alt+a is still ESC a', 1, [press(A, ALT)], hex(ESC, 'a'));
  M('ctrl+1', 1, [press(ONE, CTRL)], mok(49, 5));
  M('ctrl+;', 1, [press(SEMI, CTRL)], mok(59, 5));
  M('ctrl+alt+1', 1, [press(ONE, CTRL_ALT)], mok(49, 7));
  M('meta+a', 1, [press(A, META)], mok(97, 9));
  M('shift+enter is still a carriage return', 1, [press(ENTER, SHIFT)], hex('\r'));
  M('ctrl+space is still NUL', 1, [press(SPACE, CTRL)], hex(0x00));
  M('plain a', 1, [press(A)], hex('a'));
  // Level 0 is the legacy form.
  M('ctrl+1 sends 1', 0, [press(ONE, CTRL)], hex('1'));
  return rows;
}

// ---------------------------------------------------------------------------------------------
// The mouse.

type Encoding = 'legacy' | 'utf8' | 'urxvt' | 'sgr';
type Mode = 'x10' | 'click' | 'drag' | 'motion';

const MODE_NAME: Record<Mode, string> = {
  x10: 'mouse_x10',
  click: 'mouse_1000',
  drag: 'mouse_1002',
  motion: 'mouse_1003',
};
const ENCODING_MODE: Record<Encoding, string[]> = {
  legacy: [],
  utf8: ['mouse_utf8'],
  urxvt: ['mouse_urxvt'],
  sgr: ['mouse_sgr'],
};

interface MouseSpec {
  kind: 'down' | 'up' | 'move' | 'wheel';
  button?: 'left' | 'middle' | 'right';
  col: number;
  row: number;
  mods?: Mods;
  wheelX?: number;
  wheelY?: number;
}

const BUTTON = { left: 0, middle: 1, right: 2 } as const;

/** The bytes of one value in the legacy form (+32) or the UTF-8 form. */
function legacyValue(value: number, utf8: boolean): number[] | undefined {
  const v = value + 32;
  if (!utf8) return v <= 255 ? [v] : undefined;
  if (v < 0x80) return [v];
  return v <= 0x7ff ? [0xc0 | (v >> 6), 0x80 | (v & 0x3f)] : undefined;
}

/** The parts of a report for `hex`: text, and raw bytes as numbers. */
type Parts = (string | number)[];

/** One report: `button` is the whole button value of ctlseqs, including motion and modifiers. */
function report(enc: Encoding, button: number, col: number, row: number, release: boolean): Parts {
  const x = col + 1;
  const y = row + 1;
  if (enc === 'sgr') return [`${CSI}<${button};${x};${y}${release ? 'm' : 'M'}`];
  if (enc === 'urxvt') return [`${CSI}${button + 32};${x};${y}M`];
  const values = [button, x, y].map((v) => legacyValue(v, enc === 'utf8'));
  if (values.some((v) => v === undefined)) return [];
  return [`${CSI}M`, ...values.flatMap((v) => v as number[])];
}

/** The bytes of a mouse event from the tables of ctlseqs, "Mouse Tracking". */
function mouseBytes(mode: Mode, enc: Encoding, e: MouseSpec): Parts {
  const m = e.mods ?? {};
  const modifiers = mode === 'x10' ? 0 : (m.shift ? 4 : 0) + (m.alt ? 8 : 0) + (m.ctrl ? 16 : 0);
  const button = e.button ? BUTTON[e.button] : undefined;
  if (e.kind === 'down') {
    return button === undefined ? [] : report(enc, button + modifiers, e.col, e.row, false);
  }
  if (e.kind === 'up') {
    if (button === undefined || mode === 'x10') return [];
    const code = enc === 'sgr' ? button : 3;
    return report(enc, code + modifiers, e.col, e.row, enc === 'sgr');
  }
  if (e.kind === 'move') {
    const reported = mode === 'motion' || (mode === 'drag' && button !== undefined);
    return reported ? report(enc, 32 + (button ?? 3) + modifiers, e.col, e.row, false) : [];
  }
  if (mode === 'x10') return [];
  const out: Parts = [];
  for (const [notches, negative, positive] of [
    [e.wheelY ?? 0, 64, 65],
    [e.wheelX ?? 0, 66, 67],
  ] as const) {
    for (let i = 0; i < Math.min(Math.abs(notches), 16); i++) {
      out.push(
        ...report(enc, (notches < 0 ? negative : positive) + modifiers, e.col, e.row, false),
      );
    }
  }
  return out;
}

const mouseRow = (e: MouseSpec): Row => ({
  type: 'mouse',
  kind: e.kind,
  button: e.button,
  col: e.col,
  row: e.row,
  mods: modNames(e.mods ?? {}),
  wheel_x: e.wheelX || undefined,
  wheel_y: e.wheelY || undefined,
});

function mouseCase(
  name: string,
  mode: Mode,
  enc: Encoding,
  events: MouseSpec[],
  extraModes: string[] = [],
): Row {
  return {
    name: `mouse ${mode}, ${enc}: ${name}`,
    modes: [MODE_NAME[mode], ...ENCODING_MODE[enc], ...extraModes],
    logical: events.map(mouseRow),
    expect: hex(...events.flatMap((e) => mouseBytes(mode, enc, e))),
    source: 'xterm ctlseqs: Mouse Tracking, modes 9, 1000, 1002, 1003, 1005, 1006, 1015',
  };
}

function mouse(): Row[] {
  const rows: Row[] = [];
  const events: [string, MouseSpec[]][] = [
    ['left press', [{ kind: 'down', button: 'left', col: 0, row: 0 }]],
    [
      'left press and release',
      [
        { kind: 'down', button: 'left', col: 9, row: 4 },
        { kind: 'up', button: 'left', col: 9, row: 4 },
      ],
    ],
    ['middle press', [{ kind: 'down', button: 'middle', col: 9, row: 4 }]],
    [
      'right press and release',
      [
        { kind: 'down', button: 'right', col: 79, row: 23 },
        { kind: 'up', button: 'right', col: 79, row: 23 },
      ],
    ],
    ['ctrl+left press', [{ kind: 'down', button: 'left', col: 1, row: 1, mods: CTRL }]],
    ['shift+alt+right press', [{ kind: 'down', button: 'right', col: 1, row: 1, mods: SHIFT_ALT }]],
    [
      'shift+alt+ctrl+middle release',
      [{ kind: 'up', button: 'middle', col: 1, row: 1, mods: ALL }],
    ],
    ['wheel up', [{ kind: 'wheel', col: 5, row: 5, wheelY: -1 }]],
    ['wheel down by 3', [{ kind: 'wheel', col: 5, row: 5, wheelY: 3 }]],
    [
      'wheel left and right',
      [
        { kind: 'wheel', col: 5, row: 5, wheelX: -1 },
        { kind: 'wheel', col: 5, row: 5, wheelX: 2 },
      ],
    ],
    ['wheel with ctrl', [{ kind: 'wheel', col: 0, row: 0, wheelY: -1, mods: CTRL }]],
    ['wheel by 40 is cut at 16', [{ kind: 'wheel', col: 0, row: 0, wheelY: 40 }]],
    ['left drag', [{ kind: 'move', button: 'left', col: 3, row: 4 }]],
    ['motion without a button', [{ kind: 'move', col: 3, row: 4 }]],
    ['column 199 (x 200)', [{ kind: 'down', button: 'left', col: 199, row: 0 }]],
    [
      'column 222 (x 223), the last one of the legacy form',
      [{ kind: 'down', button: 'left', col: 222, row: 0 }],
    ],
    ['column 223 (x 224)', [{ kind: 'down', button: 'left', col: 223, row: 0 }]],
    ['row 299', [{ kind: 'down', button: 'left', col: 0, row: 299 }]],
    [
      'column 2014 (x 2015), the last one of the UTF-8 form',
      [{ kind: 'down', button: 'left', col: 2014, row: 0 }],
    ],
    ['column 2015 (x 2016)', [{ kind: 'down', button: 'left', col: 2015, row: 0 }]],
  ];
  for (const enc of ['legacy', 'utf8', 'urxvt', 'sgr'] as Encoding[]) {
    for (const [name, list] of events) rows.push(mouseCase(name, 'motion', enc, list));
  }
  // The modes differ in what they report.
  const probes: [string, MouseSpec[]][] = [
    ['press', [{ kind: 'down', button: 'left', col: 2, row: 3, mods: CTRL }]],
    ['release', [{ kind: 'up', button: 'left', col: 2, row: 3 }]],
    ['wheel', [{ kind: 'wheel', col: 2, row: 3, wheelY: -1 }]],
    ['drag', [{ kind: 'move', button: 'left', col: 2, row: 3 }]],
    ['motion', [{ kind: 'move', col: 2, row: 3 }]],
  ];
  for (const mode of ['x10', 'click', 'drag'] as Mode[]) {
    for (const [name, list] of probes) rows.push(mouseCase(name, mode, 'sgr', list));
  }
  for (const [name, list] of probes) rows.push(mouseCase(name, 'x10', 'legacy', list));
  // Several modes at once: the one that reports most wins, and SGR wins over the others.
  rows.push({
    name: 'mouse: 1003 wins over 1000, SGR wins over urxvt',
    modes: ['mouse_1000', 'mouse_1003', 'mouse_urxvt', 'mouse_sgr'],
    logical: [mouseRow({ kind: 'move', col: 0, row: 0 })],
    expect: hex(`${CSI}<35;1;1M`),
    source: 'neohar: 1003 > 1002 > 1000 > 9 and SGR > urxvt > UTF-8 when several are set',
  });
  // Without a mouse mode the core keeps the event; alternate scroll turns the wheel into arrows.
  rows.push({
    name: 'mouse: no mouse mode, the core keeps a click',
    logical: [mouseRow({ kind: 'down', button: 'left', col: 0, row: 0 })],
    delivery: 'local',
    expect: '',
    source: 'neohar: the core uses the event for selection and scrolling',
  });
  rows.push({
    name: 'mouse: no mouse mode, the core keeps the wheel on the primary screen',
    modes: ['alternate_scroll'],
    logical: [mouseRow({ kind: 'wheel', col: 0, row: 0, wheelY: 1 })],
    delivery: 'local',
    expect: '',
    source: 'xterm ctlseqs, alternate scroll (1007) applies on the alternate screen only',
  });
  for (const app of [false, true]) {
    for (const [name, wheel, final] of [
      ['up', -2, 'A'],
      ['down', 3, 'B'],
    ] as const) {
      rows.push({
        name: `mouse: alternate scroll, wheel ${name}${app ? ', application cursor' : ''}`,
        modes: ['alt_screen', 'alternate_scroll', ...(app ? ['app_cursor'] : [])],
        logical: [mouseRow({ kind: 'wheel', col: 0, row: 0, wheelY: wheel })],
        expect: hex((app ? `${SS3}${final}` : `${CSI}${final}`).repeat(Math.abs(wheel))),
        source: 'xterm ctlseqs, alternate scroll (1007): the wheel sends cursor keys',
      });
    }
  }
  rows.push({
    name: 'mouse: a mouse mode wins over alternate scroll',
    modes: ['alt_screen', 'alternate_scroll', 'mouse_1000', 'mouse_sgr'],
    logical: [mouseRow({ kind: 'wheel', col: 0, row: 0, wheelY: -1 })],
    expect: hex(`${CSI}<64;1;1M`),
    source: 'xterm ctlseqs, alternate scroll (1007): it applies only when no mouse mode is on',
  });
  return rows;
}

// ---------------------------------------------------------------------------------------------

/** The generated files: file name to the text of the file. */
export function generate(): Record<string, string> {
  const files: Record<string, Row[]> = {
    'gen-text.toml': [
      ...legacyText(LETTERS, 'letter'),
      ...legacyText(DIGIT_SPECS, 'digit'),
      ...legacyText(PUNCTUATION, 'punctuation'),
    ],
    'gen-navigation.toml': navigation(),
    'gen-function.toml': functionKeys(),
    'gen-special.toml': specialKeys(),
    'gen-keypad.toml': keypad(),
    'gen-kitty.toml': kitty(),
    'gen-modify-other-keys.toml': modifyOtherKeys(),
    'gen-mouse.toml': mouse(),
  };
  const header =
    '# Generated by tests/keyboard/generate.ts. Do not edit; change the script and run it again.\n\n';
  return Object.fromEntries(
    Object.entries(files).map(([name, rows]) => [
      name,
      `${header}${rows.map((row) => table('case', row)).join('\n\n')}\n`,
    ]),
  );
}

if (import.meta.main) {
  const dir = join(import.meta.dir, 'cases');
  for (const [name, text] of Object.entries(generate())) {
    await Bun.write(join(dir, name), text);
    console.log(`${name}: ${text.split('[[case]]').length - 1} cases`);
  }
}
