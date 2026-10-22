// The size of a cell, from the font.

import { context2d, makeCanvas } from './canvas';
import type { CellMetrics, FontSpec } from './types';

/** What the browser measures for a font, in device pixels. */
export interface FontMeasure {
  /** The width of one character of the monospace font, not rounded. */
  advance: number;
  /** From the baseline up to the top of the font's line. */
  ascent: number;
  /** From the baseline down to the bottom of the font's line. */
  descent: number;
}

/** The CSS font of a variant. Bit 0 of `variant` is bold and bit 1 is italic. */
export function fontString(font: FontSpec, variant: number): string {
  const italic = variant & 2 ? 'italic ' : '';
  const bold = variant & 1 ? 'bold ' : '';
  return `${italic}${bold}${font.sizePx}px ${font.family}`;
}

/**
 * Integer metrics from a measurement. The cell is as wide as the rounded advance and as high as
 * the rounded-up line. The text sits in the middle of the extra height. Underline, strikethrough
 * and the cursor bar are as thick as a fourteenth of the font size, but never thinner than one
 * pixel.
 */
export function deriveMetrics(measure: FontMeasure, sizePx: number): CellMetrics {
  const cellW = Math.max(1, Math.round(measure.advance));
  const content = measure.ascent + measure.descent;
  const cellH = Math.max(3, Math.ceil(content));
  const lineH = Math.max(1, Math.round(sizePx / 14));
  const baseline = clamp(Math.round(measure.ascent + (cellH - content) / 2), 1, cellH - 1);
  const underlineY = clamp(
    baseline + Math.max(1, Math.round(sizePx * 0.1)),
    0,
    Math.max(0, cellH - lineH),
  );
  const strikeY = clamp(Math.round(baseline - sizePx * 0.3 - lineH / 2), 0, cellH - lineH);
  return { cellW, cellH, baseline, underlineY, lineH, strikeY };
}

function clamp(value: number, low: number, high: number): number {
  return Math.max(low, Math.min(high, value));
}

/**
 * Measures `font` with a canvas. It uses 32 characters to get the advance with a fraction, and
 * the font's own bounding box for the line, so that the height does not depend on the letters.
 */
export function measureFont(font: FontSpec): FontMeasure {
  const ctx = context2d(makeCanvas(8, 8), true);
  ctx.font = fontString(font, 0);
  const advance = ctx.measureText('M'.repeat(32)).width / 32;
  const m = ctx.measureText('Mg');
  const ascent = m.fontBoundingBoxAscent ?? m.actualBoundingBoxAscent;
  const descent = m.fontBoundingBoxDescent ?? m.actualBoundingBoxDescent;
  return { advance, ascent, descent };
}

/** The cell metrics of a font. */
export function cellMetrics(font: FontSpec): CellMetrics {
  return deriveMetrics(measureFont(font), font.sizePx);
}
