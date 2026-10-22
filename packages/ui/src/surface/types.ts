// The surface: how the pixels of the panes are painted.
//
// A window has one canvas. A renderer paints every visible pane of the window into its rectangle.
// The renderer reads the cells, styles and cursor from the decoder (`@neohar/proto-wasm`) and
// keeps no terminal state of its own: whatever it holds is a copy for the GPU.

import type { Decoder } from '@neohar/proto-wasm';

/** The decoder as the renderers use it. */
export type { Decoder };

/** A rectangle in device pixels of the window canvas. All four numbers are integers. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * What the canvas paints around the cells: the ground of the window, the main panel that
 * holds the panes, and the round corners of a pane. Colours are `0xRRGGBB`, and lengths are in
 * device pixels.
 */
export interface Chrome {
  ground: number;
  /** The main panel, or null when the window has none. */
  frame: Rect | null;
  frameColor: number;
  frameRadius: number;
  /** The radius of the bottom corners of a pane. Its strip, in the DOM, has the top ones. */
  paneRadius: number;
}

/** The chrome of a window that has not told the renderer its own: black, square, no panel. */
export const PLAIN_CHROME: Chrome = {
  ground: 0x000000,
  frame: null,
  frameColor: 0x000000,
  frameRadius: 0,
  paneRadius: 0,
};

/** A size in device pixels. */
export interface Size {
  w: number;
  h: number;
}

/** The font of the terminal cells. `sizePx` is in device pixels, so it includes the pixel ratio. */
export interface FontSpec {
  family: string;
  sizePx: number;
}

/**
 * The size of a cell and the places inside it, in device pixels. All are integers, so that a cell
 * covers whole pixels and the atlas can be sampled without filtering.
 */
export interface CellMetrics {
  cellW: number;
  cellH: number;
  /** From the top of the cell to the baseline of the text. */
  baseline: number;
  /** From the top of the cell to the top of the underline. */
  underlineY: number;
  /** The thickness of underlines, strikethrough and the bar and outline of the cursor. */
  lineH: number;
  /** From the top of the cell to the top of the strikethrough. */
  strikeY: number;
}

/** A selection as the renderer draws it: cells of the visible grid, in reading order. */
export interface SelectionCells {
  /** 0 none, 1 stream of cells, 2 rectangle. */
  mode: number;
  startCol: number;
  startRow: number;
  endCol: number;
  endRow: number;
}

/**
 * A selection from the core, in lines of the whole scrollback. `mode` is 1 simple, 2 word, 3
 * line, 4 block (the number that `selection_mode` returns).
 */
export interface SelectionLines {
  mode: number;
  startCol: number;
  startLine: number;
  endCol: number;
  endLine: number;
}

/** Which renderer paints. */
export type RendererKind = 'webgl2' | 'canvas2d';

/** What the host can know about the renderer that runs (`window.report_metrics`). */
export interface RendererInfo {
  kind: RendererKind;
  /** The name of the GPU, when the browser says it. */
  gpu: string | null;
}

/**
 * One renderer for a window. The host calls these in this order for each frame of a pane:
 * `decoder.decode(frame)`, then `applyDecoded(pane)` when the status is applied, and later
 * `draw(now)` once per display frame. `applyDecoded` must follow every applied decode, because
 * the decoder keeps the glyphs to rasterize and the rows to upload for the last frame only.
 */
export interface SurfaceRenderer {
  readonly info: RendererInfo;
  /** The metrics of the current font. */
  readonly metrics: CellMetrics;
  /** How often the atlas was emptied and filled again (a full atlas, a new font, a lost context). */
  readonly atlasResets: number;
  /** True while a cursor blinks, so the host must keep calling `draw` on every display frame. */
  readonly blinking: boolean;
  /** Sets the font. Measures the cells again, empties the atlas and repaints every pane. */
  setFont(font: FontSpec): CellMetrics;
  attachPane(id: number): void;
  detachPane(id: number): void;
  /**
   * Places pane `id`: its cells start at the top left of `rect`, and its background fills
   * `outer` (the box with the padding), which is `rect` when it is not given.
   */
  setRect(id: number, rect: Rect, outer?: Rect): void;
  /** Sets the ground, the main panel and the corners of the panes, and paints everything again. */
  setChrome(chrome: Chrome): void;
  /** Takes what the last decoded frame changed in pane `id`: rows, styles, glyphs, cursor. */
  applyDecoded(id: number): void;
  /** Overrides the selection of a pane. `applyDecoded` sets it from the frame. */
  setSelection(id: number, selection: SelectionLines | null): void;
  /**
   * The part of the grid of pane `id` that its box showed in the last paint: the first column
   * and row. It is 0 and 0 unless the grid is larger than the box.
   */
  crop(id: number): { left: number; top: number };
  /** Moves the part of a larger grid by `left` columns and `top` rows (two fingers on a phone). */
  setPan(id: number, left: number, top: number): void;
  /** Resizes the canvas. */
  resize(canvasPx: Size): void;
  /**
   * Paints the panes when something changed since the last call, or when a blink changed phase.
   * `now` is a time in milliseconds, for example the argument of `requestAnimationFrame`.
   * Returns whether it painted.
   */
  draw(now: number): boolean;
  destroy(): void;
}

/** How the host hears of a renderer that lost its resources and has them again. */
export interface SurfaceEvents {
  /** The GPU context came back and every pane was uploaded again from the decoder. */
  onContextRestored?: () => void;
}
