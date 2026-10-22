// The contrast of the colours of the chrome (WCAG 2.1): each pair of text and background
// that the chrome uses has 4.5 or more, and the border of a field has 3 or more (1.4.11). The test
// reads the custom properties of `tokens.css`, so a new default that fails cannot merge. It also
// checks that the keys of the theme equal the defaults that the core sends, and that glass is
// opaque enough for text over the worst backdrop.

import { describe, expect, test } from 'bun:test';

const tokens = await Bun.file(new URL('tokens.css', import.meta.url)).text();
const defaults = await Bun.file(
  new URL('../../../../crates/nh-core/src/config/defaults.toml', import.meta.url),
).text();

type Rgb = [number, number, number];

/** The value of each custom property, with its comment removed. */
const declared = new Map(
  [...tokens.matchAll(/--([a-z-]+):\s*([^;]+);/g)].map((match) => [match[1]!, match[2]!.trim()]),
);

function hex(value: string): Rgb {
  const digits = /^#([0-9a-f]{6})$/i.exec(value)?.[1];
  if (!digits) throw new Error(`not a colour of six digits: ${value}`);
  return [0, 2, 4].map((at) => parseInt(digits.slice(at, at + 2), 16) / 255) as Rgb;
}

const linear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gamma = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

function oklab([r, g, b]: Rgb): Rgb {
  const [lr, lg, lb] = [linear(r), linear(g), linear(b)];
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function srgb([L, a, b]: Rgb): Rgb {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const clip = (c: number) => Math.min(1, Math.max(0, gamma(c)));
  return [
    clip(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    clip(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    clip(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

/** A colour of the tokens: six digits, `var(--x)`, or `color-mix(in oklch, A p%, B)`. */
function resolve(value: string): Rgb {
  const reference = /^var\(--([a-z-]+)\)$/.exec(value);
  if (reference) return token(reference[1]!);
  const mix = /^color-mix\(in oklch, (.+) (\d+)%, (.+)\)$/.exec(value);
  if (mix) {
    // One end is gray in the mixes that this test reads. A gray has no hue in OKLCH, so the mix
    // keeps the hue of the other end and scales its chroma, as a mix in Oklab does.
    const share = Number(mix[2]) / 100;
    const [a, b] = [oklab(resolve(mix[1]!)), oklab(resolve(mix[3]!))];
    return srgb(a.map((part, at) => part * share + b[at]! * (1 - share)) as Rgb);
  }
  return hex(value.split(/\s/)[0]!);
}

function token(name: string): Rgb {
  const value = declared.get(name);
  if (value === undefined) throw new Error(`tokens.css has no --${name}`);
  return resolve(value);
}

function contrast(one: Rgb, two: Rgb): number {
  const luminance = ([r, g, b]: Rgb) =>
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
  const [light, dark] = [luminance(one), luminance(two)].sort((x, y) => y - x) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

/** Text and its background, as the chrome uses them. */
const TEXT: [string, string][] = [
  ['text', 'bg'],
  ['text', 'bg-raised'],
  ['text', 'bg-hover'],
  ['text', 'cap'],
  ['text-dim', 'bg'],
  ['text-dim', 'bg-raised'],
  ['text-dim', 'bg-hover'],
  ['text', 'bg-card'],
  ['text-dim', 'bg-card'],
  ['text', 'tint-caution'],
  ['text-dim', 'tint-caution'],
  ['text', 'tint-danger'],
  ['text-dim', 'tint-danger'],
  ['text', 'bg-focus'],
  ['text-dim', 'bg-focus'],
  ['live', 'bg-focus'],
  ['text', 'tint-caution-focus'],
  ['text-dim', 'tint-caution-focus'],
  ['caution', 'tint-caution-focus'],
  ['text', 'tint-danger-focus'],
  ['text-dim', 'tint-danger-focus'],
  ['danger', 'tint-danger-focus'],
  ['caution', 'tint-caution'],
  ['danger', 'tint-danger'],
  ['live', 'bg-card'],
  ['danger', 'tint-caution'],
  ['caution', 'tint-danger'],
  ['accent', 'tint-caution'],
  ['accent', 'tint-danger'],
  ['danger', 'bg-card'],
  ['caution', 'bg-card'],
  ['live', 'bg'],
  ['live', 'bg-raised'],
  ['live', 'bg-hover'],
  ['accent', 'bg'],
  ['accent', 'bg-raised'],
  ['caution', 'bg'],
  ['caution', 'bg-raised'],
  ['danger', 'bg'],
  ['danger', 'bg-raised'],
  ['danger', 'bg-hover'],
  ['on-signal', 'live'],
  ['on-signal', 'accent'],
  ['on-signal', 'caution'],
  ['on-signal', 'danger'],
  ['on-here', 'here'],
  ['syntax-keyword', 'bg'],
  ['syntax-string', 'bg'],
  ['syntax-number', 'bg'],
  ['syntax-function', 'bg'],
  ['syntax-keyword', 'bg-raised'],
  ['syntax-string', 'bg-raised'],
  ['syntax-number', 'bg-raised'],
  ['syntax-function', 'bg-raised'],
];

/** The share of `--bg-raised` in `--glass`, in percent. */
function glassShare(): number {
  const share = /^color-mix\(in oklch, var\(--bg-raised\) (\d+)%, transparent\)$/.exec(
    declared.get('glass') ?? '',
  )?.[1];
  if (!share) throw new Error('--glass is not a share of --bg-raised over transparent');
  return Number(share);
}

/** Glass of `share` percent over a backdrop, as the browser blends it: in sRGB. */
function glassOver(share: number, backdrop: Rgb): Rgb {
  const glass = token('bg-raised');
  return glass.map((part, at) => (part * share) / 100 + backdrop[at]! * (1 - share / 100)) as Rgb;
}

/** The text on glass. The worst backdrop is a pane with a light background: the text colour. */
const ON_GLASS = ['text', 'text-dim'];

/** The border of a field and the ground beside it. */
const FIELD: [string, string][] = [
  ['text-dim', 'bg'],
  ['text-dim', 'bg-raised'],
];

describe('the colours of the chrome', () => {
  test.each(TEXT)('--%s on --%s has a contrast of 4.5 or more', (fore, back) => {
    expect(contrast(token(fore), token(back))).toBeGreaterThanOrEqual(4.5);
  });

  test.each(FIELD)('a field border of --%s on --%s has 3 or more', (border, back) => {
    expect(contrast(token(border), token(back))).toBeGreaterThanOrEqual(3);
  });

  test.each(ON_GLASS)('--%s on glass has 4.5 or more over the worst backdrop', (fore) => {
    const worst = glassOver(glassShare(), token('text'));
    expect(contrast(token(fore), worst)).toBeGreaterThanOrEqual(4.5);
  });

  test('glass is not more opaque than it must be: two points less fails', () => {
    const worst = glassOver(glassShare() - 2, token('text'));
    expect(Math.min(...ON_GLASS.map((fore) => contrast(token(fore), worst)))).toBeLessThan(4.5);
  });

  test('the rule is not a field border: --line has less than 3 on black', () => {
    expect(contrast(token('line'), token('bg'))).toBeLessThan(3);
  });

  test('the keys of the theme equal the defaults of the core', () => {
    const section = /\n\[theme\]\n([\s\S]*?)\n\[/.exec(defaults)?.[1] ?? '';
    const keys = [...section.matchAll(/^([a-z_]+) = "(#[0-9a-f]{6})"$/gm)];
    expect(keys.map((key) => key[1])).toEqual([
      'bg',
      'bg_raised',
      'bg_hover',
      'line',
      'text',
      'text_dim',
      'accent',
      'danger',
    ]);
    for (const [, key, value] of keys) {
      expect(declared.get(key!.replaceAll('_', '-'))?.split(/\s/)[0]).toBe(value);
    }
  });

  test('the formula gives the ratios of WCAG', () => {
    expect(contrast(hex('#ffffff'), hex('#000000'))).toBeCloseTo(21, 5);
    expect(contrast(hex('#777777'), hex('#ffffff'))).toBeCloseTo(4.48, 2);
  });
});
