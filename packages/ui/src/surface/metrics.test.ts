import { expect, test } from 'bun:test';
import { deriveMetrics, fontString } from './metrics';

test('the cell is the rounded advance and the rounded-up line', () => {
  const m = deriveMetrics({ advance: 10.8, ascent: 26.1, descent: 7.3 }, 28);
  expect(m.cellW).toBe(11);
  expect(m.cellH).toBe(34);
  for (const value of Object.values(m)) expect(Number.isInteger(value)).toBe(true);
});

test('the baseline lies inside the cell, with the extra height shared above and below', () => {
  const m = deriveMetrics({ advance: 9, ascent: 14, descent: 4.4 }, 18);
  expect(m.cellH).toBe(19);
  expect(m.baseline).toBeGreaterThan(0);
  expect(m.baseline).toBeLessThan(m.cellH);
  expect(m.baseline).toBe(14);
});

test('lines are at least one pixel thick and stay inside the cell', () => {
  for (const size of [6, 9, 13, 18, 26, 40, 72]) {
    const m = deriveMetrics(
      { advance: size * 0.6, ascent: size * 0.95, descent: size * 0.25 },
      size,
    );
    expect(m.lineH).toBeGreaterThanOrEqual(1);
    expect(m.underlineY + m.lineH).toBeLessThanOrEqual(m.cellH);
    expect(m.strikeY + m.lineH).toBeLessThanOrEqual(m.cellH);
    expect(m.strikeY).toBeLessThan(m.baseline);
    expect(m.underlineY).toBeGreaterThanOrEqual(m.baseline);
  }
});

test('a font without height still gives a usable cell', () => {
  const m = deriveMetrics({ advance: 0, ascent: 0, descent: 0 }, 1);
  expect(m.cellW).toBe(1);
  expect(m.cellH).toBeGreaterThanOrEqual(3);
  expect(m.baseline).toBeGreaterThanOrEqual(1);
});

test('the variant bits choose bold and italic', () => {
  const font = { family: 'Menlo, monospace', sizePx: 24 };
  expect(fontString(font, 0)).toBe('24px Menlo, monospace');
  expect(fontString(font, 1)).toBe('bold 24px Menlo, monospace');
  expect(fontString(font, 2)).toBe('italic 24px Menlo, monospace');
  expect(fontString(font, 3)).toBe('italic bold 24px Menlo, monospace');
});
