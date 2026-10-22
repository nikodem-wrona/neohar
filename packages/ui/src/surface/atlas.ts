// The glyph atlas: rasterizing the glyphs that the decoder queues.
//
// The decoder decides which slot a glyph gets (`AtlasMap`). The renderer draws each new glyph once,
// with a 2D canvas, so that the browser does font fallback, emoji and hinting. The result is a
// bitmap of one cell with white text on a transparent background, or the colours of an emoji.
// The renderer puts the bitmap in its own storage: a texture for WebGL2, a canvas for Canvas2D.

import { type AnyCanvas, type Ctx2D, context2d, makeCanvas } from './canvas';
import { ATLAS_SLOTS, type AtlasGeometry, atlasGeometry } from './geometry';
import { fontString } from './metrics';
import type { CellMetrics, Decoder, FontSpec } from './types';

/** The high bit of a glyph that is a cluster of several code points. */
const CLUSTER = 0x8000_0000;

/** Where the rasterized glyphs go. */
export interface GlyphSink {
  /**
   * `bitmap` holds the glyph for `slot`: exactly one cell, white text with an alpha channel, or
   * colours when `color` is true. It is valid only during the call.
   */
  put(slot: number, bitmap: AnyCanvas, color: boolean): void;
}

/** Draws glyphs into a canvas of one cell. */
export class GlyphRasterizer {
  readonly canvas: AnyCanvas;
  private readonly ctx: Ctx2D;
  private current = '';

  constructor(
    private readonly font: FontSpec,
    private readonly metrics: CellMetrics,
  ) {
    this.canvas = makeCanvas(metrics.cellW, metrics.cellH);
    this.ctx = context2d(this.canvas, true);
    this.ctx.textBaseline = 'alphabetic';
    this.ctx.fillStyle = '#fff';
  }

  /**
   * Draws `text` so that column `half` of it fills the canvas (0 for a glyph of one cell and for
   * the left half of a wide glyph, 1 for the right half). Returns true when the bitmap has colours
   * of its own, as an emoji has.
   */
  draw(text: string, variant: number, half: number): boolean {
    const { cellW, cellH, baseline } = this.metrics;
    const ctx = this.ctx;
    const css = fontString(this.font, variant);
    if (css !== this.current) {
      ctx.font = css;
      this.current = css;
    }
    ctx.clearRect(0, 0, cellW, cellH);
    ctx.fillText(text, -half * cellW, baseline);
    return hasColor(ctx.getImageData(0, 0, cellW, cellH).data);
  }
}

/**
 * Whether the pixels have a colour other than white. The glyphs of text are drawn white, so only
 * a glyph with colours of its own (an emoji) fails the test.
 */
export function hasColor(rgba: Uint8ClampedArray): boolean {
  for (let i = 0; i < rgba.length; i += 4) {
    if (rgba[i + 3]! > 32 && (rgba[i]! < 200 || rgba[i + 1]! < 200 || rgba[i + 2]! < 200)) {
      return true;
    }
  }
  return false;
}

/** The atlas for one font: its layout, and the rasterizer that fills it. */
export class Atlas {
  readonly geometry: AtlasGeometry;
  private readonly rasterizer: GlyphRasterizer;
  private readonly text = new TextDecoder();

  constructor(
    private readonly decoder: Decoder,
    font: FontSpec,
    readonly metrics: CellMetrics,
    slots: number = ATLAS_SLOTS,
  ) {
    this.geometry = atlasGeometry(metrics.cellW, metrics.cellH, slots);
    this.rasterizer = new GlyphRasterizer(font, metrics);
  }

  /**
   * Rasterizes the glyphs that the last decoded frame queued, and hands each bitmap to `sink`.
   * The decoder clears the list at the next `decode`, so this must run after every decode.
   * Returns the number of glyphs.
   */
  flushPending(sink: GlyphSink): number {
    const raw = this.decoder.raw;
    const count = raw.pending_len();
    for (let i = 0; i < count; i++) {
      const slot = raw.pending_field(i, 0);
      const glyph = raw.pending_field(i, 1);
      const variant = raw.pending_field(i, 2);
      const wide = raw.pending_field(i, 3) !== 0;
      const text = this.glyphText(glyph);
      if (!wide) {
        const color = this.rasterizer.draw(text, variant, 0);
        sink.put(slot, this.rasterizer.canvas, color);
        continue;
      }
      // A wide glyph has two slots, one for each half. Both halves tell the shader the same kind,
      // so find out first whether any half has colours of its own.
      const rightColor = this.rasterizer.draw(text, variant, 1);
      const leftColor = this.rasterizer.draw(text, variant, 0);
      const color = leftColor || rightColor;
      sink.put(slot, this.rasterizer.canvas, color);
      this.rasterizer.draw(text, variant, 1);
      sink.put(slot + 1, this.rasterizer.canvas, color);
    }
    return count;
  }

  private glyphText(glyph: number): string {
    if (glyph >= CLUSTER) {
      return this.text.decode(this.decoder.atlasText(glyph));
    }
    return String.fromCodePoint(glyph);
  }
}
