// The GPU state of one pane, and what it takes from the decoder.
//
// A pane has three data textures: the cell words (`R32UI`, one texel per cell), the style ids
// (`R16UI`) and the styles (`RGBA32UI`, 256 per row). `applyDecoded` only marks what changed and
// reads the small things (cursor, colours, selection). `upload` copies the marked rows at draw
// time, from the arrays of the decoder, so that several frames that arrive between two display
// frames cost one upload.

import { forEachRun, PaneCrop } from '../geometry';
import { emptyRows, emptySelection, linesToRows, rowsToCells } from '../selection';
import type { Decoder, Rect, SelectionCells, SelectionLines } from '../types';
import { UNIT } from './shaders';

/** The attribute bit of blinking text. */
const ATTR_BLINK = 64;
/** Styles in a row of the style texture. */
const STYLES_PER_ROW = 256;

function dataTexture(
  gl: WebGL2RenderingContext,
  unit: number,
  internal: number,
  w: number,
  h: number,
): WebGLTexture {
  const texture = gl.createTexture();
  if (!texture) throw new Error('the GPU gave no texture');
  gl.activeTexture(gl.TEXTURE0 + unit);
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texStorage2D(gl.TEXTURE_2D, 1, internal, w, h);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return texture;
}

/** Writes the channels of `0xRRGGBB` to the first three places of `out`, as 0 to 1. */
export function rgb(color: number, out: Float32Array): void {
  out[0] = ((color >> 16) & 255) / 255;
  out[1] = ((color >> 8) & 255) / 255;
  out[2] = (color & 255) / 255;
}

/** One pane of a window. */
export class GlPane {
  cols = 0;
  rows = 0;
  readonly rect: Rect = { x: 0, y: 0, w: 0, h: 0 };
  /** The box of the background: the rectangle of the cells and the padding around it. */
  readonly outer: Rect = { x: 0, y: 0, w: 0, h: 0 };
  /** The selection as rows of the grid. Mode 0: none. */
  private readonly selectionRows = emptyRows();
  /** The selection as cells of the grid, for the shader. */
  readonly selection: SelectionCells = emptySelection();
  /** The default background of the pane, as `0xRRGGBB`. */
  background = 0;
  /** The same as red, green, blue and alpha between 0 and 1, for the shader of the shapes. */
  readonly backgroundRgba = new Float32Array([0, 0, 0, 1]);
  cursorColor = 0xffffff;
  /** The cursor colour as red, green and blue between 0 and 1, for `uniform3fv`. */
  readonly cursorRgb = new Float32Array([1, 1, 1]);
  cursorVisible = false;
  cursorBlink = false;
  /** 0 block, 1 underline, 2 bar, 3 hollow block. */
  cursorShape = 0;
  /** The cell of the cursor, for the crop of a larger grid. */
  readonly cursorAt = { x: 0, y: 0, visible: false };
  /** The part of a larger grid that the box shows. */
  readonly view = new PaneCrop();
  /** Whether some style of the pane has blinking text. */
  blinkText = false;

  private cells: WebGLTexture | null = null;
  private ids: WebGLTexture | null = null;
  private styles: WebGLTexture | null = null;
  private textureCols = 0;
  private textureRows = 0;
  private styleRows = 0;
  private dirty = new Uint8Array(0);
  private all = true;
  private cellsView: Uint32Array = new Uint32Array(0);
  private idsView: Uint16Array = new Uint16Array(0);
  // One function for the life of the pane, so that the walk over runs allocates nothing.
  private readonly uploadRun = (first: number, count: number) => this.uploadRows(first, count);

  constructor(
    readonly id: number,
    private readonly gl: WebGL2RenderingContext,
    private readonly decoder: Decoder,
  ) {}

  /** Every row needs uploading again (a new atlas, a new context, a new size). */
  markAll(): void {
    this.all = true;
  }

  /** The textures are gone with the context. */
  contextLost(): void {
    this.cells = this.ids = this.styles = null;
    this.textureCols = this.textureRows = this.styleRows = 0;
    this.all = true;
  }

  /** Takes what the last decoded frame changed. */
  sync(): void {
    const raw = this.decoder.raw;
    const id = this.id;
    const cols = raw.cols(id);
    const rows = raw.rows(id);
    if (cols !== this.cols || rows !== this.rows) {
      this.cols = cols;
      this.rows = rows;
      this.dirty = new Uint8Array(rows);
      this.all = true;
    }
    if (raw.pane() === id && !this.all) {
      const count = raw.dirty_rows_len();
      for (let i = 0; i < count; i++) {
        const row = raw.dirty_row(i);
        if (row < rows) this.dirty[row] = 1;
      }
    }
    // A number that is not an integer passed to a WebGL call is boxed on the heap in each frame,
    // so the colours go to the GPU as typed arrays that change only when the colour does.
    const background = raw.palette_bg(id);
    if (background !== this.background) {
      this.background = background;
      rgb(background, this.backgroundRgba);
    }
    const cursor = raw.palette_cursor(id);
    if (cursor !== this.cursorColor) {
      this.cursorColor = cursor;
      rgb(cursor, this.cursorRgb);
    }
    this.cursorVisible = raw.cursor_visible(id);
    this.cursorBlink = raw.cursor_blink(id);
    this.cursorShape = raw.cursor_shape(id);
    this.cursorAt.x = raw.cursor_x(id);
    this.cursorAt.y = raw.cursor_y(id);
    this.cursorAt.visible = this.cursorVisible;
    this.syncSelection();
  }

  /** Takes the selection from the decoder. */
  syncSelection(): void {
    const raw = this.decoder.raw;
    const id = this.id;
    const s = this.selectionRows;
    s.mode = raw.selection_mode(id);
    s.startCol = raw.selection_start_col(id);
    s.endCol = raw.selection_end_col(id);
    s.startRow = raw.selection_start_row(id);
    s.endRow = raw.selection_end_row(id);
    rowsToCells(s, this.cols, this.rows, this.selection);
  }

  /** Overrides the selection with one in lines of the scrollback. `null` clears it. */
  setSelection(selection: SelectionLines | null): void {
    const top = this.decoder.raw.viewport_top(this.id);
    linesToRows(selection, top, this.rows, this.selectionRows);
    rowsToCells(this.selectionRows, this.cols, this.rows, this.selection);
  }

  /** Copies the rows that changed to the GPU. */
  upload(): void {
    if (this.cols === 0 || this.rows === 0) return;
    this.ensureTextures();
    this.uploadStyles();
    if (this.all) {
      this.dirty.fill(1);
      this.all = false;
    }
    this.cellsView = this.decoder.cells(this.id);
    this.idsView = this.decoder.styles(this.id);
    forEachRun(this.dirty, this.rows, this.uploadRun);
    this.dirty.fill(0);
  }

  /** Binds the three textures to their units. */
  bind(): void {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0 + UNIT.cells);
    gl.bindTexture(gl.TEXTURE_2D, this.cells);
    gl.activeTexture(gl.TEXTURE0 + UNIT.ids);
    gl.bindTexture(gl.TEXTURE_2D, this.ids);
    gl.activeTexture(gl.TEXTURE0 + UNIT.styles);
    gl.bindTexture(gl.TEXTURE_2D, this.styles);
  }

  /** Frees the textures. */
  dispose(): void {
    const gl = this.gl;
    gl.deleteTexture(this.cells);
    gl.deleteTexture(this.ids);
    gl.deleteTexture(this.styles);
    this.cells = this.ids = this.styles = null;
  }

  private ensureTextures(): void {
    const gl = this.gl;
    if (this.cells && this.textureCols === this.cols && this.textureRows === this.rows) return;
    gl.deleteTexture(this.cells);
    gl.deleteTexture(this.ids);
    this.cells = dataTexture(gl, UNIT.cells, gl.R32UI, this.cols, this.rows);
    this.ids = dataTexture(gl, UNIT.ids, gl.R16UI, this.cols, this.rows);
    this.textureCols = this.cols;
    this.textureRows = this.rows;
    this.all = true;
  }

  private uploadRows(first: number, count: number): void {
    const gl = this.gl;
    const offset = first * this.cols;
    gl.activeTexture(gl.TEXTURE0 + UNIT.cells);
    gl.bindTexture(gl.TEXTURE_2D, this.cells);
    gl.texSubImage2D(
      gl.TEXTURE_2D,
      0,
      0,
      first,
      this.cols,
      count,
      gl.RED_INTEGER,
      gl.UNSIGNED_INT,
      this.cellsView,
      offset,
    );
    gl.activeTexture(gl.TEXTURE0 + UNIT.ids);
    gl.bindTexture(gl.TEXTURE_2D, this.ids);
    gl.texSubImage2D(
      gl.TEXTURE_2D,
      0,
      0,
      first,
      this.cols,
      count,
      gl.RED_INTEGER,
      gl.UNSIGNED_SHORT,
      this.idsView,
      offset,
    );
  }

  private uploadStyles(): void {
    const raw = this.decoder.raw;
    const id = this.id;
    const gl = this.gl;
    if (this.styles && !raw.style_table_dirty(id)) return;
    const count = raw.style_table_len(id);
    const needed = Math.max(1, Math.ceil(count / STYLES_PER_ROW));
    if (!this.styles || needed > this.styleRows) {
      gl.deleteTexture(this.styles);
      this.styleRows = Math.min(STYLES_PER_ROW, 2 ** Math.ceil(Math.log2(needed)));
      this.styles = dataTexture(gl, UNIT.styles, gl.RGBA32UI, STYLES_PER_ROW, this.styleRows);
    }
    gl.activeTexture(gl.TEXTURE0 + UNIT.styles);
    gl.bindTexture(gl.TEXTURE_2D, this.styles);
    const table = this.decoder.styleTable(id);
    const full = Math.floor(count / STYLES_PER_ROW);
    if (full > 0) {
      gl.texSubImage2D(
        gl.TEXTURE_2D,
        0,
        0,
        0,
        STYLES_PER_ROW,
        full,
        gl.RGBA_INTEGER,
        gl.UNSIGNED_INT,
        table,
        0,
      );
    }
    const rest = count - full * STYLES_PER_ROW;
    if (rest > 0) {
      gl.texSubImage2D(
        gl.TEXTURE_2D,
        0,
        0,
        full,
        rest,
        1,
        gl.RGBA_INTEGER,
        gl.UNSIGNED_INT,
        table,
        full * STYLES_PER_ROW * 4,
      );
    }
    let blink = false;
    for (let i = 0; i < count; i++) {
      if ((table[i * 4 + 3]! & ATTR_BLINK) !== 0) {
        blink = true;
        break;
      }
    }
    this.blinkText = blink;
    raw.style_table_clean(id);
  }
}
