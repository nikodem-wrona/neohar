import { expect, test } from 'bun:test';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { generate, hex, label } from './generate';

const casesDir = join(import.meta.dir, 'cases');

test('the generated files on disk are what the script writes', () => {
  for (const [name, text] of Object.entries(generate())) {
    expect(
      readFileSync(join(casesDir, name), 'utf8'),
      `${name}: run bun tests/keyboard/generate.ts`,
    ).toBe(text);
  }
  const onDisk = readdirSync(casesDir).filter((f) => f.startsWith('gen-'));
  expect(onDisk.sort()).toEqual(Object.keys(generate()).sort());
});

// ------------------------------------------------------------------------------------------
// The expected bytes of the legacy keys against the terminfo entry of xterm-256color, which is
// not our table: it comes from ncurses. The check is skipped on a machine without `infocmp`.

/** The text of a terminfo capability as bytes: `\E`, `^H`, `\\` and octal escapes. */
function decode(cap: string): string {
  let out = '';
  for (let i = 0; i < cap.length; i++) {
    const c = cap.charAt(i);
    if (c === '\\') {
      const next = cap.charAt(++i);
      if (next === 'E' || next === 'e') out += '\x1b';
      else if (next === 'n') out += '\n';
      else if (next === 'r') out += '\r';
      else if (next === 't') out += '\t';
      else if (next === 's') out += ' ';
      else if (/[0-7]/.test(next)) {
        let digits = next;
        while (digits.length < 3 && /[0-7]/.test(cap.charAt(i + 1))) digits += cap.charAt(++i);
        out += String.fromCharCode(parseInt(digits, 8));
      } else out += next;
    } else if (c === '^') {
      const next = cap.charAt(++i);
      out += next === '?' ? '\x7f' : String.fromCharCode(next.charCodeAt(0) & 0x1f);
    } else out += c;
  }
  return out;
}

function terminfo(): Map<string, string> | null {
  try {
    const run = Bun.spawnSync(['infocmp', '-1', '-x', 'xterm-256color']);
    if (run.exitCode !== 0) return null;
    const caps = new Map<string, string>();
    for (const line of run.stdout.toString().split('\n')) {
      const m = /^\s+([A-Za-z0-9]+)=(.*),$/.exec(line);
      if (m?.[1] && m[2] !== undefined) caps.set(m[1], decode(m[2]));
    }
    return caps.has('kcuu1') ? caps : null;
  } catch {
    return null;
  }
}

const caps = terminfo();

/** The `expect` of the generated cases, by name. */
function expectations(): Map<string, string> {
  const map = new Map<string, string>();
  for (const [file, text] of Object.entries(generate())) {
    if (file === 'gen-text.toml') continue;
    const parsed = Bun.TOML.parse(text) as { case: { name: string; expect: string }[] };
    for (const c of parsed.case) map.set(c.name, c.expect);
  }
  return map;
}

const SHIFT = { shift: true };
const ALT = { alt: true };
const SHIFT_ALT = { shift: true, alt: true };
const CTRL = { ctrl: true };
const CTRL_SHIFT = { ctrl: true, shift: true };
const CTRL_ALT = { ctrl: true, alt: true };

test.skipIf(caps === null)('the legacy keys equal the terminfo entry of xterm-256color', () => {
  const have = expectations();
  const checked: string[] = [];
  const check = (cap: string, caseName: string) => {
    const value = caps?.get(cap);
    expect(value, `terminfo has no ${cap}`).toBeDefined();
    expect(have.get(caseName), `${cap} is ${JSON.stringify(value)}; case ${caseName}`).toBe(
      hex(value as string),
    );
    checked.push(cap);
  };
  // Application cursor mode is what `smkx` turns on, so the cursor keys of terminfo are SS3.
  for (const [cap, key] of [
    ['kcuu1', 'ArrowUp'],
    ['kcud1', 'ArrowDown'],
    ['kcuf1', 'ArrowRight'],
    ['kcub1', 'ArrowLeft'],
    ['khome', 'Home'],
    ['kend', 'End'],
  ] as const) {
    check(cap, `${key} plain, application cursor`);
  }
  for (const [cap, key] of [
    ['kich1', 'Insert'],
    ['kdch1', 'Delete'],
    ['kpp', 'PageUp'],
    ['knp', 'PageDown'],
  ] as const) {
    check(cap, `${key} plain`);
  }
  check('kcbt', 'Tab shift');
  check('kent', 'NumpadEnter application keypad');
  for (let n = 1; n <= 12; n++) check(`kf${n}`, `F${n} plain`);
  // Shift is the capability without a number. The numbers are the modifier parameter.
  const params: [string, object][] = [
    ['', SHIFT],
    ['3', ALT],
    ['4', SHIFT_ALT],
    ['5', CTRL],
    ['6', CTRL_SHIFT],
    ['7', CTRL_ALT],
  ];
  for (const [prefix, key] of [
    ['kUP', 'ArrowUp'],
    ['kDN', 'ArrowDown'],
    ['kRIT', 'ArrowRight'],
    ['kLFT', 'ArrowLeft'],
    ['kHOM', 'Home'],
    ['kEND', 'End'],
    ['kIC', 'Insert'],
    ['kDC', 'Delete'],
    ['kNXT', 'PageDown'],
    ['kPRV', 'PageUp'],
  ] as const) {
    for (const [suffix, mods] of params) {
      check(`${prefix}${suffix}`, `${key} ${label(mods)}`);
    }
  }
  // kf13 to kf63 are F1 to F12 with a modifier: shift, ctrl, ctrl+shift, alt, alt+shift.
  const blocks: object[] = [SHIFT, CTRL, CTRL_SHIFT, ALT, SHIFT_ALT];
  for (const [block, mods] of blocks.entries()) {
    for (let n = 1; n <= 12; n++) {
      const cap = `kf${12 * (block + 1) + n}`;
      if (caps?.has(cap)) check(cap, `F${n} ${label(mods)}`);
    }
  }
  expect(checked.length).toBeGreaterThan(120);
});
