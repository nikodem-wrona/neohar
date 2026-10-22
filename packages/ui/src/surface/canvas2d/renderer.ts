// The Canvas2D renderer: the fallback for webviews without a usable WebGL.
//
// It has the same interface as the WebGL2 renderer and reads the same arrays from the decoder. It
// paints dirty rows only. Each glyph comes from the same bitmaps as in WebGL2: the atlas pages
// are canvases, and a glyph of text is tinted with the foreground colour before it is drawn. The
// formulas for dim text, the selection, underlines and the cursor are the ones of the shaders, so
// that the two renderers paint the same pixels (up to the rounding of the compositor). A change of
// the boxes or of the chrome paints the whole canvas again: the ground, the main panel and the
// rounded background of each pane.

import { Atlas, type GlyphSink } from '../atlas';
import { type AnyCanvas, type Ctx2D, context2d, makeCanvas } from '../canvas';
import { ATLAS_SLOTS, forEachRun, NO_CROP, PaneCrop, type SlotPlace, slotPlace } from '../geometry';
import { blinkPhase } from '../blink';
import { cellMetrics } from '../metrics';
import {
  emptyRows,
  emptySelection,
  linesToRows,
  rowsToCells,
  SELECT_RECT,
  SELECT_STREAM,
} from '../selection';
import {
  type CellMetrics,
  type Chrome,
  type Decoder,
  type FontSpec,
  PLAIN_CHROME,
  type Rect,
  type RendererInfo,
  type SelectionCells,
  type SelectionLines,
  type Size,
  type SurfaceEvents,
  type SurfaceRenderer,
} from '../types';

const FLAG_WIDE = 1 << 24;
const FLAG_SPACER = 1 << 25;
const FLAG_CURSOR = 1 << 26;
const ATTR_DIM = 2;
const ATTR_STRIKE = 16;
const ATTR_HIDDEN = 32;
const ATTR_BLINK = 64;

export interface Canvas2dOptions {
  font: FontSpec;
  events?: SurfaceEvents;
  gpu?: string | null;
  selectionColor?: readonly [number, number, number, number];
  /** The number of slots of the atlas. It must be the number the decoder was made with. */
  slots?: number;
}

class Pane2d {
  cols = 0;
  rows = 0;
  readonly rect: Rect = { x: 0, y: 0, w: 0, h: 0 };
  /** The box of the background: the rectangle of the cells and the padding around it. */
  readonly outer: Rect = { x: 0, y: 0, w: 0, h: 0 };
  dirty = new Uint8Array(0);
  /** The whole pane is painted again, background included. */
  all = true;
  readonly selectionRows = emptyRows();
  readonly selection: SelectionCells = emptySelection();
  background = 0;
  cursorColor = 0xffffff;
  cursorVisible = false;
  cursorBlink = false;
  cursorShape = 0;
  cursorX = 0;
  cursorY = 0;
  /** The cell of the cursor, for the crop of a larger grid. */
  readonly cursorAt = { x: 0, y: 0, visible: false };
  /** The part of a larger grid that the box shows. */
  readonly view = new PaneCrop();
  /** Where the first cell of the grid is on the canvas: the box, moved by the crop. */
  originX = 0;
  originY = 0;
  blinkText = false;
  constructor(readonly id: number) {}
}

/** A canvas of tinted glyphs and the places of the ones it holds. */
interface TintCache {
  canvas: AnyCanvas;
  ctx: Ctx2D;
  perRow: number;
  capacity: number;
  used: number;
  /** The index of a pair of a slot and a colour. */
  index: Map<number, number>;
}

/** The size of a cache canvas, in pixels. */
const TINT_PX = 2048;

function makeTintCache(cellW: number, cellH: number): TintCache {
  const perRow = Math.max(1, Math.floor(TINT_PX / cellW));
  const rows = Math.max(1, Math.floor(TINT_PX / cellH));
  const canvas = makeCanvas(perRow * cellW, rows * cellH);
  return {
    canvas,
    ctx: context2d(canvas),
    perRow,
    capacity: perRow * rows,
    used: 0,
    index: new Map(),
  };
}

/** The renderer of a window on a 2D canvas. */
export class Canvas2dRenderer implements SurfaceRenderer {
  readonly info: RendererInfo;
  metrics: CellMetrics;
  blinking = false;
  /** How often the atlas was emptied and filled again. For tests. */
  atlasResets = 0;

  private readonly ctx: Ctx2D;
  private readonly selectionColor: readonly [number, number, number, number];
  private font: FontSpec;
  private atlas: Atlas;
  private readonly pages: Array<{ canvas: AnyCanvas; ctx: Ctx2D } | undefined> = [];
  private readonly slots: number;
  private readonly kinds: Uint8Array;
  private tint: AnyCanvas;
  private tintCtx: Ctx2D;
  /** Glyphs of text that are already tinted: one cell for each pair of a slot and a colour. */
  private tinted = makeTintCache(1, 1);
  private readonly panes = new Map<number, Pane2d>();
  private readonly list: Pane2d[] = [];
  private readonly colors = new Map<number, string>();
  private readonly place: SlotPlace = { page: 0, x: 0, y: 0 };
  private phaseOn = true;
  private dirty = true;
  /** The boxes or the chrome changed: the whole canvas is painted again. */
  private whole = true;
  private chrome: Chrome = PLAIN_CHROME;
  /** The radii of a pane, for `roundRect`. The array is made once. */
  private readonly radii = [0, 0, 0, 0];
  private current: Pane2d | null = null;
  private readonly sink: GlyphSink = {
    put: (slot: number, bitmap: AnyCanvas, color: boolean) => this.putGlyph(slot, bitmap, color),
  };
  private readonly paintRun = (first: number, count: number) => this.paintRows(first, count);

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly decoder: Decoder,
    options: Canvas2dOptions,
  ) {
    this.ctx = context2d(canvas);
    this.info = { kind: 'canvas2d', gpu: options.gpu ?? null };
    this.selectionColor = options.selectionColor ?? [0.35, 0.55, 0.95, 0.45];
    this.font = options.font;
    this.slots = options.slots ?? ATLAS_SLOTS;
    this.kinds = new Uint8Array(this.slots);
    this.metrics = cellMetrics(this.font);
    this.atlas = new Atlas(decoder, this.font, this.metrics, this.slots);
    this.tint = makeCanvas(this.metrics.cellW, this.metrics.cellH);
    this.tintCtx = context2d(this.tint);
    this.tinted = makeTintCache(this.metrics.cellW, this.metrics.cellH);
  }

  setFont(font: FontSpec): CellMetrics {
    this.font = font;
    this.metrics = cellMetrics(font);
    this.atlas = new Atlas(this.decoder, font, this.metrics, this.slots);
    this.tint = makeCanvas(this.metrics.cellW, this.metrics.cellH);
    this.tintCtx = context2d(this.tint);
    this.tinted = makeTintCache(this.metrics.cellW, this.metrics.cellH);
    this.pages.length = 0;
    this.resetAtlas();
    return this.metrics;
  }

  attachPane(id: number): void {
    if (this.panes.has(id)) return;
    const pane = new Pane2d(id);
    this.panes.set(id, pane);
    this.list.push(pane);
    this.dirty = true;
  }

  detachPane(id: number): void {
    const pane = this.panes.get(id);
    if (!pane) return;
    this.panes.delete(id);
    this.list.splice(this.list.indexOf(pane), 1);
    this.refreshBlinking();
    this.dirty = true;
  }

  setRect(id: number, rect: Rect, outer: Rect = rect): void {
    const pane = this.panes.get(id);
    if (!pane) return;
    pane.rect.x = rect.x;
    pane.rect.y = rect.y;
    pane.rect.w = rect.w;
    pane.rect.h = rect.h;
    pane.outer.x = outer.x;
    pane.outer.y = outer.y;
    pane.outer.w = outer.w;
    pane.outer.h = outer.h;
    this.whole = true;
    this.dirty = true;
  }

  setChrome(chrome: Chrome): void {
    this.chrome = { ...chrome, frame: chrome.frame ? { ...chrome.frame } : null };
    this.whole = true;
    this.dirty = true;
  }

  applyDecoded(id: number): void {
    this.atlas.flushPending(this.sink);
    if (this.decoder.raw.atlas_full()) this.resetAtlas();
    const pane = this.panes.get(id);
    if (!pane) return;
    const raw = this.decoder.raw;
    const cols = raw.cols(id);
    const rows = raw.rows(id);
    if (cols !== pane.cols || rows !== pane.rows) {
      pane.cols = cols;
      pane.rows = rows;
      pane.dirty = new Uint8Array(rows);
      pane.all = true;
    }
    if (raw.pane() === id && !pane.all) {
      const count = raw.dirty_rows_len();
      for (let i = 0; i < count; i++) {
        const row = raw.dirty_row(i);
        if (row < rows) pane.dirty[row] = 1;
      }
    }
    // The cursor moves between rows: both rows are in the dirty list of the frame. A change of
    // the selection or the colours can touch any row.
    const s = pane.selectionRows;
    const mode = raw.selection_mode(id);
    const startCol = raw.selection_start_col(id);
    const startRow = raw.selection_start_row(id);
    const endCol = raw.selection_end_col(id);
    const endRow = raw.selection_end_row(id);
    const changed =
      mode !== s.mode ||
      (mode !== 0 &&
        (s.startCol !== startCol ||
          s.endCol !== endCol ||
          s.startRow !== startRow ||
          s.endRow !== endRow));
    s.mode = mode;
    s.startCol = startCol;
    s.startRow = startRow;
    s.endCol = endCol;
    s.endRow = endRow;
    rowsToCells(s, cols, rows, pane.selection);
    const background = raw.palette_bg(id);
    if (changed || background !== pane.background || raw.style_table_dirty(id)) pane.all = true;
    // The padding has the background too, and it has round corners.
    if (background !== pane.background) this.whole = true;
    pane.background = background;
    pane.cursorColor = raw.palette_cursor(id);
    pane.cursorVisible = raw.cursor_visible(id);
    pane.cursorBlink = raw.cursor_blink(id);
    pane.cursorShape = raw.cursor_shape(id);
    pane.cursorX = raw.cursor_x(id);
    pane.cursorY = raw.cursor_y(id);
    if (raw.style_table_dirty(id)) {
      this.scanBlink(pane);
      raw.style_table_clean(id);
    }
    this.refreshBlinking();
    this.dirty = true;
  }

  crop(id: number): { left: number; top: number } {
    return this.panes.get(id)?.view.crop ?? NO_CROP;
  }

  setPan(id: number, left: number, top: number): void {
    const pane = this.panes.get(id);
    if (!pane || (pane.view.pan.left === left && pane.view.pan.top === top)) return;
    pane.view.pan.left = left;
    pane.view.pan.top = top;
    this.dirty = true;
  }

  setSelection(id: number, selection: SelectionLines | null): void {
    const pane = this.panes.get(id);
    if (!pane) return;
    linesToRows(selection, this.decoder.raw.viewport_top(id), pane.rows, pane.selectionRows);
    rowsToCells(pane.selectionRows, pane.cols, pane.rows, pane.selection);
    pane.all = true;
    this.dirty = true;
  }

  resize(canvasPx: Size): void {
    if (this.canvas.width === canvasPx.w && this.canvas.height === canvasPx.h) return;
    this.canvas.width = canvasPx.w;
    this.canvas.height = canvasPx.h;
    this.whole = true;
    this.dirty = true;
  }

  draw(now: number): boolean {
    const phaseOn = blinkPhase(now);
    const flipped = this.blinking && phaseOn !== this.phaseOn;
    if (!this.dirty && !flipped) return false;
    this.phaseOn = phaseOn;
    this.dirty = false;
    if (this.whole) this.paintChrome();
    for (let i = 0; i < this.list.length; i++) {
      const pane = this.list[i]!;
      if (flipped) {
        if (pane.blinkText) pane.all = true;
        else if (pane.cursorBlink && pane.cursorY < pane.rows) pane.dirty[pane.cursorY] = 1;
      }
      this.paintPane(pane);
    }
    return true;
  }

  destroy(): void {
    this.panes.clear();
    this.list.length = 0;
    this.pages.length = 0;
  }

  private paintPane(pane: Pane2d): void {
    const rect = pane.rect;
    if (rect.w <= 0 || rect.h <= 0 || pane.cols === 0 || pane.rows === 0) return;
    const ctx = this.ctx;
    // A grid that is larger than its box shows the part around the cursor. A new part
    // paints the whole pane again.
    pane.cursorAt.x = pane.cursorX;
    pane.cursorAt.y = pane.cursorY;
    pane.cursorAt.visible = pane.cursorVisible;
    const m = this.metrics;
    if (pane.view.place(rect, m.cellW, m.cellH, pane, pane.cursorAt)) pane.all = true;
    pane.originX = rect.x - pane.view.crop.left * m.cellW;
    pane.originY = rect.y - pane.view.crop.top * m.cellH;
    if (pane.all) {
      // The rectangle of the cells only: the padding keeps it off the round corners, and a fill of
      // the curve again would add to the edge pixels, which are partly transparent.
      ctx.fillStyle = this.color(pane.background);
      ctx.fillRect(rect.x, rect.y, rect.w, rect.h);
      pane.dirty.fill(1);
      pane.all = false;
    }
    pane.cursorY = Math.min(pane.cursorY, pane.rows - 1);
    this.current = pane;
    ctx.save();
    ctx.beginPath();
    ctx.rect(rect.x, rect.y, rect.w, rect.h);
    ctx.clip();
    forEachRun(pane.dirty, pane.rows, this.paintRun);
    ctx.restore();
    pane.dirty.fill(0);
  }

  /** Paints the ground and the main panel, and marks every pane to be painted again. */
  private paintChrome(): void {
    this.whole = false;
    const ctx = this.ctx;
    const chrome = this.chrome;
    ctx.fillStyle = this.color(chrome.ground);
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
    const frame = chrome.frame;
    if (frame) {
      ctx.fillStyle = this.color(chrome.frameColor);
      ctx.beginPath();
      ctx.roundRect(frame.x, frame.y, frame.w, frame.h, chrome.frameRadius);
      ctx.fill();
    }
    // Only the bottom corners of a pane are round: its strip, in the DOM, has the top ones.
    this.radii[2] = this.radii[3] = chrome.paneRadius;
    for (let i = 0; i < this.list.length; i++) {
      const pane = this.list[i]!;
      const outer = pane.outer;
      if (outer.w > 0 && outer.h > 0) {
        ctx.fillStyle = this.color(pane.background);
        ctx.beginPath();
        ctx.roundRect(outer.x, outer.y, outer.w, outer.h, this.radii);
        ctx.fill();
      }
      pane.all = true;
    }
  }

  private paintRows(first: number, count: number): void {
    for (let y = first; y < first + count; y++) this.paintRow(this.current!, y);
  }

  private paintRow(pane: Pane2d, y: number): void {
    const id = pane.id;
    const m = this.metrics;
    const cells = this.decoder.cells(id);
    const ids = this.decoder.styles(id);
    const table = this.decoder.styleTable(id);
    const ctx = this.ctx;
    const [selR, selG, selB, selA] = this.selectionColor;
    const sel = pane.selection;
    const cursorOn = pane.cursorVisible && (!pane.cursorBlink || this.phaseOn);
    const top = pane.originY + y * m.cellH;
    // The row is painted over the background of the pane, so clear it first.
    ctx.fillStyle = this.color(pane.background);
    ctx.fillRect(pane.originX, top, pane.cols * m.cellW, m.cellH);
    for (let x = 0; x < pane.cols; x++) {
      const cell = cells[y * pane.cols + x]!;
      if ((cell & FLAG_SPACER) !== 0) continue;
      const styleId = ids[y * pane.cols + x]!;
      const base = styleId * 4;
      let fg = table[base]! >>> 8;
      let bg = table[base + 1]! >>> 8;
      const ul = table[base + 2]! >>> 8;
      const attr = table[base + 3]! & 255;
      const underline = (table[base + 3]! >>> 8) & 7;
      const hidden = (attr & ATTR_HIDDEN) !== 0;
      if ((attr & ATTR_DIM) !== 0) fg = mixRgb(bg, fg, 0.6);
      let selected = false;
      if (sel.mode === SELECT_STREAM) {
        selected =
          (y > sel.startRow || (y === sel.startRow && x >= sel.startCol)) &&
          (y < sel.endRow || (y === sel.endRow && x <= sel.endCol));
      } else if (sel.mode === SELECT_RECT) {
        selected = y >= sel.startRow && y <= sel.endRow && x >= sel.startCol && x <= sel.endCol;
      }
      if (selected) bg = mixTo(bg, selR, selG, selB, selA);
      const wide = (cell & FLAG_WIDE) !== 0;
      let primitive = 0;
      if ((cell & FLAG_CURSOR) !== 0 && cursorOn) {
        if (pane.cursorShape === 0) {
          fg = bg;
          bg = pane.cursorColor;
        } else {
          primitive = pane.cursorShape;
        }
      }
      const left = pane.originX + x * m.cellW;
      const width = wide ? m.cellW * 2 : m.cellW;
      ctx.fillStyle = this.color(bg);
      ctx.fillRect(left, top, width, m.cellH);
      const slot = cell & 0xffffff;
      const blinkOff = (attr & ATTR_BLINK) !== 0 && !this.phaseOn;
      if (slot !== 0 && !hidden && !blinkOff) {
        this.paintGlyph(slot, fg, left, top, this.kinds[slot] === 1);
        if (wide) this.paintGlyph(slot + 1, fg, left + m.cellW, top, this.kinds[slot] === 1);
      }
      if (!hidden) {
        if (underline !== 0) this.paintUnderline(underline, ul, left, top, width);
        if ((attr & ATTR_STRIKE) !== 0) {
          ctx.fillStyle = this.color(fg);
          ctx.fillRect(left, top + m.strikeY, width, m.lineH);
        }
      }
      if (primitive !== 0) this.paintCursor(primitive, pane.cursorColor, left, top, width);
    }
  }

  private paintGlyph(slot: number, fg: number, x: number, y: number, color: boolean): void {
    const g = this.atlas.geometry;
    const page = this.pages[slotPlace(g, slot, this.place).page];
    if (!page) return;
    if (color) {
      const { x: sx, y: sy } = this.place;
      this.ctx.drawImage(page.canvas, sx, sy, g.cellW, g.cellH, x, y, g.cellW, g.cellH);
      return;
    }
    // A glyph of text is white in the atlas. It is tinted once for each colour, and the tinted
    // cell stays in a cache, because changing the state of a canvas for each cell is slow.
    const cache = this.tinted;
    const key = slot * 0x1000000 + fg;
    let index = cache.index.get(key);
    if (index === undefined) {
      if (cache.used === cache.capacity) {
        cache.index.clear();
        cache.used = 0;
      }
      index = cache.used++;
      cache.index.set(key, index);
      this.tintInto(cache, index, page.canvas, fg);
    }
    const cx = (index % cache.perRow) * g.cellW;
    const cy = Math.floor(index / cache.perRow) * g.cellH;
    this.ctx.drawImage(cache.canvas, cx, cy, g.cellW, g.cellH, x, y, g.cellW, g.cellH);
  }

  /** Draws the glyph at `this.place` of an atlas page in colour `fg`, into entry `index`. */
  private tintInto(cache: TintCache, index: number, page: AnyCanvas, fg: number): void {
    const g = this.atlas.geometry;
    const t = this.tintCtx;
    t.globalCompositeOperation = 'copy';
    t.drawImage(page, this.place.x, this.place.y, g.cellW, g.cellH, 0, 0, g.cellW, g.cellH);
    t.globalCompositeOperation = 'source-in';
    t.fillStyle = this.color(fg);
    t.fillRect(0, 0, g.cellW, g.cellH);
    const cx = (index % cache.perRow) * g.cellW;
    const cy = Math.floor(index / cache.perRow) * g.cellH;
    cache.ctx.clearRect(cx, cy, g.cellW, g.cellH);
    cache.ctx.drawImage(this.tint, cx, cy);
  }

  private paintUnderline(kind: number, rgb: number, left: number, top: number, width: number) {
    const m = this.metrics;
    const ctx = this.ctx;
    const lh = Math.max(m.lineH, 1);
    ctx.fillStyle = this.color(rgb);
    const uy = m.underlineY;
    if (kind === 1) {
      ctx.fillRect(left, top + uy, width, lh);
    } else if (kind === 2) {
      const y1 = Math.min(uy, m.cellH - 3 * lh);
      ctx.fillRect(left, top + y1, width, lh);
      ctx.fillRect(left, top + y1 + 2 * lh, width, lh);
    } else if (kind === 3) {
      // The same integer arithmetic as the fragment shader.
      const amp = Math.max(1, lh);
      const base16 = 16 * Math.min(uy, m.cellH - lh - amp - 1);
      const cw = m.cellW;
      const quarter = Math.floor((cw * cw) / 4);
      for (let px = 0; px < width; px++) {
        const v = (2 * px) % cw;
        const bump = v * (cw - v);
        const signedBump = (2 * px) % (2 * cw) < cw ? bump : -bump;
        const wave16 = Math.floor(((signedBump + quarter) * amp * 32) / (cw * cw));
        const center16 = base16 + wave16 - 8 * amp;
        for (let py = 0; py < m.cellH; py++) {
          if (Math.abs(16 * py + 8 - center16 - 8 * lh) < 8 * lh + 8) {
            ctx.fillRect(left + px, top + py, 1, 1);
          }
        }
      }
    } else if (kind === 4) {
      for (let px = 0; px < width; px += lh * 2) {
        ctx.fillRect(left + px, top + uy, Math.min(lh, width - px), lh);
      }
    } else {
      const gap = Math.max(Math.floor(m.cellW / 4), 1);
      for (let px = 0; px < width; px += m.cellW) {
        ctx.fillRect(left + px, top + uy, Math.min(m.cellW - gap, width - px), lh);
      }
    }
  }

  private paintCursor(shape: number, rgb: number, left: number, top: number, width: number) {
    const m = this.metrics;
    const lh = Math.max(m.lineH, 1);
    const ctx = this.ctx;
    ctx.fillStyle = this.color(rgb);
    if (shape === 1) {
      const t = Math.max(lh, Math.floor(m.cellH / 8));
      ctx.fillRect(left, top + m.cellH - t, width, t);
    } else if (shape === 2) {
      ctx.fillRect(left, top, Math.max(lh, Math.floor(m.cellW / 8)), m.cellH);
    } else {
      ctx.fillRect(left, top, width, lh);
      ctx.fillRect(left, top + m.cellH - lh, width, lh);
      ctx.fillRect(left, top, lh, m.cellH);
      ctx.fillRect(left + width - lh, top, lh, m.cellH);
    }
  }

  private scanBlink(pane: Pane2d): void {
    const table = this.decoder.styleTable(pane.id);
    let blink = false;
    for (let i = 3; i < table.length; i += 4) {
      if ((table[i]! & ATTR_BLINK) !== 0) {
        blink = true;
        break;
      }
    }
    pane.blinkText = blink;
  }

  private refreshBlinking(): void {
    let blinking = false;
    for (let i = 0; i < this.list.length; i++) {
      const p = this.list[i]!;
      if ((p.cursorVisible && p.cursorBlink) || p.blinkText) blinking = true;
    }
    this.blinking = blinking;
  }

  private putGlyph(slot: number, bitmap: AnyCanvas, color: boolean): void {
    const g = this.atlas.geometry;
    slotPlace(g, slot, this.place);
    let page = this.pages[this.place.page];
    if (!page) {
      const canvas = makeCanvas(g.pageW, g.pageH);
      page = { canvas, ctx: context2d(canvas) };
      this.pages[this.place.page] = page;
    }
    page.ctx.clearRect(this.place.x, this.place.y, g.cellW, g.cellH);
    page.ctx.drawImage(bitmap, this.place.x, this.place.y);
    this.kinds[slot] = color ? 1 : 0;
  }

  private resetAtlas(): void {
    this.atlasResets++;
    this.tinted.index.clear();
    this.tinted.used = 0;
    this.kinds.fill(0);
    this.decoder.raw.atlas_reset();
    this.atlas.flushPending(this.sink);
    for (let i = 0; i < this.list.length; i++) this.list[i]!.all = true;
    this.dirty = true;
  }

  /** The CSS colour of `0xRRGGBB`. Strings are made once and kept. */
  private color(rgb: number): string {
    let s = this.colors.get(rgb);
    if (s === undefined) {
      if (this.colors.size > 4096) this.colors.clear();
      s = `#${rgb.toString(16).padStart(6, '0')}`;
      this.colors.set(rgb, s);
    }
    return s;
  }
}

/** `from + (to - from) * t` for each channel, rounded like the GPU rounds. */
function mixRgb(from: number, to: number, t: number): number {
  return mixTo(from, ((to >> 16) & 255) / 255, ((to >> 8) & 255) / 255, (to & 255) / 255, t);
}

/** A colour mixed with a colour whose channels are between 0 and 1. */
function mixTo(from: number, r: number, g: number, b: number, t: number): number {
  const channel = (shift: number, target: number) =>
    Math.round(((from >> shift) & 255) * (1 - t) + target * 255 * t);
  return (channel(16, r) << 16) | (channel(8, g) << 8) | channel(0, b);
}
