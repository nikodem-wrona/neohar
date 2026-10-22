// The colours and lengths of the chrome as the renderer takes them. The canvas paints the
// ground, the main panel and the round corners of the panes, so it needs the values of the
// tokens: `0xRRGGBB` for a colour and device pixels for a radius.

import { context2d, type Ctx2D, makeCanvas } from '../surface/canvas';

let probe: HTMLElement | null = null;
let pixel: Ctx2D | null = null;
const known = new Map<string, number>();

/**
 * Any colour of CSS as `0xRRGGBB`: a value of a custom property, such as `#141414`, `oklch(…)`
 * or `color-mix(…)`. The browser resolves it on a hidden element, and a canvas of one pixel turns
 * its result into channels. The result is kept, so a value is resolved once.
 */
export function cssColor(value: string, fallback: number): number {
  const text = value.trim();
  if (text === '') return fallback;
  const found = known.get(text);
  if (found !== undefined) return found;
  if (!probe) {
    probe = document.createElement('span');
    probe.hidden = true;
    document.body.append(probe);
  }
  probe.style.color = '';
  probe.style.color = text;
  if (probe.style.color === '') return fallback;
  const resolved = getComputedStyle(probe).color;
  pixel ??= context2d(makeCanvas(1, 1), true);
  pixel.clearRect(0, 0, 1, 1);
  pixel.fillStyle = '#000000';
  pixel.fillStyle = resolved;
  pixel.fillRect(0, 0, 1, 1);
  const [r, g, b] = pixel.getImageData(0, 0, 1, 1).data;
  const color = (r! << 16) | (g! << 8) | b!;
  if (known.size > 64) known.clear();
  known.set(text, color);
  return color;
}

/** A length of CSS in pixels, such as `10px`, or 0. */
export function cssPixels(value: string): number {
  const number = parseFloat(value);
  return Number.isFinite(number) && number > 0 ? number : 0;
}
