// The WebGL2 renderer: one context per window, one instanced draw per pane. Before the
// cells, a second program draws the main panel and the background of each pane as rounded
// rectangles.

import { Atlas, type GlyphSink } from '../atlas';
import type { AnyCanvas } from '../canvas';
import { ATLAS_SLOTS, KIND_WIDTH, NO_CROP, type SlotPlace, slotPlace } from '../geometry';
import { blinkPhase } from '../blink';
import { cellMetrics } from '../metrics';
import {
  type CellMetrics,
  type Chrome,
  type Decoder,
  type FontSpec,
  PLAIN_CHROME,
  type Rect,
  type RendererInfo,
  type SelectionLines,
  type Size,
  type SurfaceEvents,
  type SurfaceRenderer,
} from '../types';
import { GlPane, rgb } from './pane';
import {
  FRAGMENT_SOURCE,
  SHAPE_FRAGMENT_SOURCE,
  SHAPE_VERTEX_SOURCE,
  UNIT,
  VERTEX_SOURCE,
} from './shaders';

/** The part of `EXT_disjoint_timer_query_webgl2` that the renderer uses. */
interface TimerExtension {
  TIME_ELAPSED_EXT: number;
  GPU_DISJOINT_EXT: number;
}

/** Timings of the last `draw`, in milliseconds, when `timing` is on. `gpuMs` is -1 until known. */
export interface DrawStats {
  /** CPU time to copy the changed rows to the GPU. */
  uploadMs: number;
  /** CPU time to issue the draw calls, without the upload. */
  drawMs: number;
  /** GPU time of the draws, from a timer query that finished some frames ago. */
  gpuMs: number;
}

/** The colour of a selection: red, green, blue and strength. */
export type SelectionColor = readonly [number, number, number, number];

export interface GlOptions {
  font: FontSpec;
  events?: SurfaceEvents;
  gpu?: string | null;
  selectionColor?: SelectionColor;
  /** The number of slots of the atlas. It must be the number the decoder was made with. */
  slots?: number;
}

interface Uniforms {
  cells: WebGLUniformLocation | null;
  ids: WebGLUniformLocation | null;
  styles: WebGLUniformLocation | null;
  kinds: WebGLUniformLocation | null;
  atlasTex: WebGLUniformLocation | null;
  grid: WebGLUniformLocation | null;
  cell: WebGLUniformLocation | null;
  origin: WebGLUniformLocation | null;
  canvas: WebGLUniformLocation | null;
  sel: WebGLUniformLocation | null;
  selMode: WebGLUniformLocation | null;
  selColor: WebGLUniformLocation | null;
  cursor: WebGLUniformLocation | null;
  cursorColor: WebGLUniformLocation | null;
  blink: WebGLUniformLocation | null;
  atlas: WebGLUniformLocation | null;
  lines: WebGLUniformLocation | null;
}

interface ShapeUniforms {
  rect: WebGLUniformLocation | null;
  radii: WebGLUniformLocation | null;
  color: WebGLUniformLocation | null;
  canvas: WebGLUniformLocation | null;
}

function link(gl: WebGL2RenderingContext, vertex: string, fragment: string): WebGLProgram {
  const program = gl.createProgram();
  if (!program) throw new Error('the GPU gave no program');
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertex));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`program does not link: ${gl.getProgramInfoLog(program) ?? ''}`);
  }
  return program;
}

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error('the GPU gave no shader');
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(`shader does not compile: ${gl.getShaderInfoLog(shader) ?? ''}`);
  }
  return shader;
}

/** The renderer of a window on WebGL2. */
export class GlRenderer implements SurfaceRenderer {
  readonly info: RendererInfo;
  metrics: CellMetrics;
  blinking = false;
  /** How often the atlas was emptied and filled again. For tests. */
  atlasResets = 0;
  /** Turns on the measurement of `stats`. It costs a few clock reads for each frame. */
  timing = false;
  readonly stats: DrawStats = { uploadMs: 0, drawMs: 0, gpuMs: -1 };

  private readonly gl: WebGL2RenderingContext;
  private timer: {
    ext: TimerExtension;
    query: WebGLQuery;
    active: boolean;
    waiting: boolean;
  } | null = null;
  private readonly events: SurfaceEvents;
  private readonly selectionRgba: Float32Array;
  /** The uniforms that do not change from frame to frame need setting. */
  private staticDirty = true;
  private font: FontSpec;
  private atlas: Atlas;
  private program: WebGLProgram | null = null;
  private shapeProgram: WebGLProgram | null = null;
  private shapeUniforms!: ShapeUniforms;
  /** The ground, the main panel and the corners of the panes. The colours are for the shaders. */
  private readonly groundRgba = new Float32Array([0, 0, 0, 1]);
  private readonly frameRgba = new Float32Array([0, 0, 0, 1]);
  private readonly frame: Rect = { x: 0, y: 0, w: 0, h: 0 };
  private hasFrame = false;
  private frameRadius = 0;
  private paneRadius = 0;
  private vao: WebGLVertexArrayObject | null = null;
  private atlasTexture: WebGLTexture | null = null;
  private kindsTexture: WebGLTexture | null = null;
  private readonly slots: number;
  private readonly kinds: Uint8Array;
  private kindsDirty = true;
  private uniforms!: Uniforms;
  private readonly panes = new Map<number, GlPane>();
  private readonly list: GlPane[] = [];
  private lost = false;
  private dirty = true;
  private phaseOn = true;
  private destroyed = false;
  private readonly place: SlotPlace = { page: 0, x: 0, y: 0 };
  private readonly sink: GlyphSink = {
    put: (slot: number, bitmap: AnyCanvas, color: boolean) => this.putGlyph(slot, bitmap, color),
  };

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly decoder: Decoder,
    options: GlOptions,
  ) {
    const gl = canvas.getContext('webgl2', {
      alpha: false,
      antialias: false,
      depth: false,
      stencil: false,
      powerPreference: 'high-performance',
      preserveDrawingBuffer: false,
    });
    if (!gl) throw new Error('the browser gives no WebGL2 context');
    this.gl = gl;
    this.events = options.events ?? {};
    this.selectionRgba = Float32Array.from(options.selectionColor ?? [0.35, 0.55, 0.95, 0.45]);
    this.info = { kind: 'webgl2', gpu: options.gpu ?? null };
    this.font = options.font;
    this.slots = options.slots ?? ATLAS_SLOTS;
    this.kinds = new Uint8Array(Math.ceil(this.slots / KIND_WIDTH) * KIND_WIDTH);
    this.metrics = cellMetrics(this.font);
    this.atlas = new Atlas(decoder, this.font, this.metrics, this.slots);
    this.setChrome(PLAIN_CHROME);
    canvas.addEventListener('webglcontextlost', this.onLost);
    canvas.addEventListener('webglcontextrestored', this.onRestored);
    this.build();
  }

  setFont(font: FontSpec): CellMetrics {
    this.font = font;
    this.metrics = cellMetrics(font);
    this.atlas = new Atlas(this.decoder, font, this.metrics, this.slots);
    this.staticDirty = true;
    if (!this.lost) {
      this.createAtlasTexture();
      this.resetAtlas();
    }
    return this.metrics;
  }

  attachPane(id: number): void {
    if (this.panes.has(id)) return;
    const pane = new GlPane(id, this.gl, this.decoder);
    this.panes.set(id, pane);
    this.list.push(pane);
    this.dirty = true;
  }

  detachPane(id: number): void {
    const pane = this.panes.get(id);
    if (!pane) return;
    pane.dispose();
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
    this.dirty = true;
  }

  setChrome(chrome: Chrome): void {
    rgb(chrome.ground, this.groundRgba);
    rgb(chrome.frameColor, this.frameRgba);
    this.hasFrame = chrome.frame !== null;
    if (chrome.frame) {
      this.frame.x = chrome.frame.x;
      this.frame.y = chrome.frame.y;
      this.frame.w = chrome.frame.w;
      this.frame.h = chrome.frame.h;
    }
    this.frameRadius = chrome.frameRadius;
    this.paneRadius = chrome.paneRadius;
    this.dirty = true;
  }

  applyDecoded(id: number): void {
    if (this.lost || this.destroyed) return;
    // The glyphs first, for every pane: the decoder lists the new glyphs of the last frame only.
    this.atlas.flushPending(this.sink);
    if (this.decoder.raw.atlas_full()) this.resetAtlas();
    const pane = this.panes.get(id);
    if (pane) {
      pane.sync();
      this.refreshBlinking();
    }
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
    pane.setSelection(selection);
    this.dirty = true;
  }

  resize(canvasPx: Size): void {
    if (this.canvas.width === canvasPx.w && this.canvas.height === canvasPx.h) return;
    // Setting the size clears the drawing buffer, so everything is painted again.
    this.canvas.width = canvasPx.w;
    this.canvas.height = canvasPx.h;
    this.staticDirty = true;
    this.dirty = true;
  }

  draw(now: number): boolean {
    if (this.lost || this.destroyed) return false;
    const phaseOn = blinkPhase(now);
    if (!this.dirty && !(this.blinking && phaseOn !== this.phaseOn)) return false;
    this.phaseOn = phaseOn;
    this.dirty = false;
    const gl = this.gl;
    const u = this.uniforms;
    const m = this.metrics;
    const width = this.canvas.width;
    const height = this.canvas.height;

    gl.viewport(0, 0, width, height);
    gl.disable(gl.SCISSOR_TEST);
    gl.clearBufferfv(gl.COLOR, 0, this.groundRgba);
    gl.bindVertexArray(this.vao);
    if (this.staticDirty) {
      gl.useProgram(this.shapeProgram);
      gl.uniform2f(this.shapeUniforms.canvas, width, height);
    }
    if (this.hasFrame) {
      const r = this.frameRadius;
      this.fillShape(this.frame, r, r, r, r, this.frameRgba);
    }

    gl.useProgram(this.program);
    this.uploadKinds();
    gl.activeTexture(gl.TEXTURE0 + UNIT.kinds);
    gl.bindTexture(gl.TEXTURE_2D, this.kindsTexture);
    gl.activeTexture(gl.TEXTURE0 + UNIT.atlas);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlasTexture);
    if (this.staticDirty) {
      // These uniforms change with the font, the size of the canvas and a new context only.
      gl.uniform2f(u.cell, m.cellW, m.cellH);
      gl.uniform2f(u.canvas, width, height);
      gl.uniform2i(u.atlas, this.atlas.geometry.perRow, this.atlas.geometry.perPage);
      gl.uniform4i(u.lines, m.underlineY, m.lineH, m.strikeY, 0);
      gl.uniform4fv(u.selColor, this.selectionRgba);
      this.staticDirty = false;
    }
    gl.uniform1i(u.blink, phaseOn ? 1 : 0);

    const timing = this.timing;
    let started = 0;
    if (timing) {
      started = performance.now();
      this.stats.uploadMs = 0;
      this.beginGpuTimer();
    }
    for (let i = 0; i < this.list.length; i++) {
      this.drawPane(this.list[i]!, height, phaseOn);
    }
    if (timing) {
      this.endGpuTimer();
      this.stats.drawMs = performance.now() - started - this.stats.uploadMs;
    }
    return true;
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.canvas.removeEventListener('webglcontextlost', this.onLost);
    this.canvas.removeEventListener('webglcontextrestored', this.onRestored);
    for (let i = 0; i < this.list.length; i++) this.list[i]!.dispose();
    this.panes.clear();
    this.list.length = 0;
    const gl = this.gl;
    gl.deleteTexture(this.atlasTexture);
    gl.deleteTexture(this.kindsTexture);
    gl.deleteVertexArray(this.vao);
    gl.deleteProgram(this.program);
    gl.deleteProgram(this.shapeProgram);
    gl.getExtension('WEBGL_lose_context')?.loseContext();
  }

  /** The first error the GPU reported since the last call, or 0. For tests. */
  glError(): number {
    const gl = this.gl;
    const first = gl.getError();
    while (gl.getError() !== gl.NO_ERROR) {
      // Read the rest, so that the next call starts clean.
    }
    return first;
  }

  /** Makes the context lose its resources and give them back. For tests. */
  loseContextForTest(): { restore: () => void } {
    const ext = this.gl.getExtension('WEBGL_lose_context');
    if (!ext) throw new Error('WEBGL_lose_context is not available');
    ext.loseContext();
    return { restore: () => ext.restoreContext() };
  }

  private drawPane(pane: GlPane, canvasHeight: number, phaseOn: boolean): void {
    const gl = this.gl;
    const u = this.uniforms;
    const rect = pane.rect;
    if (rect.w <= 0 || rect.h <= 0 || pane.cols === 0 || pane.rows === 0) return;
    if (this.timing) {
      const before = performance.now();
      pane.upload();
      this.stats.uploadMs += performance.now() - before;
    } else {
      pane.upload();
    }
    // The background fills the box with the padding. Only the bottom corners are round: the strip
    // of the pane, in the DOM, has the top ones.
    const r = this.paneRadius;
    this.fillShape(pane.outer, 0, 0, r, r, pane.backgroundRgba);
    gl.useProgram(this.program);
    gl.enable(gl.SCISSOR_TEST);
    // The scissor box has its origin at the bottom left.
    gl.scissor(rect.x, canvasHeight - rect.y - rect.h, rect.w, rect.h);
    pane.bind();
    gl.uniform2i(u.grid, pane.cols, pane.rows);
    // A grid that is larger than its box shows the part around the cursor: the cells
    // start left of and above the box, and the scissor cuts them.
    const m = this.metrics;
    pane.view.place(rect, m.cellW, m.cellH, pane, pane.cursorAt);
    const crop = pane.view.crop;
    gl.uniform2f(u.origin, rect.x - crop.left * m.cellW, rect.y - crop.top * m.cellH);
    const sel = pane.selection;
    gl.uniform4i(u.sel, sel.startCol, sel.startRow, sel.endCol, sel.endRow);
    gl.uniform1i(u.selMode, sel.mode);
    const cursorOn = pane.cursorVisible && (!pane.cursorBlink || phaseOn);
    gl.uniform2i(u.cursor, cursorOn ? 1 : 0, pane.cursorShape);
    gl.uniform3fv(u.cursorColor, pane.cursorRgb);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, pane.cols * pane.rows);
    gl.disable(gl.SCISSOR_TEST);
  }

  /** Fills a rounded rectangle with the program of the shapes. It leaves that program in use. */
  private fillShape(
    rect: Rect,
    topLeft: number,
    topRight: number,
    bottomRight: number,
    bottomLeft: number,
    color: Float32Array,
  ): void {
    if (rect.w <= 0 || rect.h <= 0) return;
    const gl = this.gl;
    const u = this.shapeUniforms;
    gl.useProgram(this.shapeProgram);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.uniform4f(u.rect, rect.x, rect.y, rect.w, rect.h);
    gl.uniform4f(u.radii, topLeft, topRight, bottomRight, bottomLeft);
    gl.uniform4f(u.color, color[0]!, color[1]!, color[2]!, 1);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.disable(gl.BLEND);
  }

  /** Starts a timer query, unless the one from an earlier frame is still not read. */
  private beginGpuTimer(): void {
    const gl = this.gl;
    if (!this.timer) {
      const ext = gl.getExtension('EXT_disjoint_timer_query_webgl2') as TimerExtension | null;
      const query = gl.createQuery();
      if (!ext || !query) return;
      this.timer = { ext, query, active: false, waiting: false };
    }
    const t = this.timer;
    if (t.waiting) {
      if (!gl.getQueryParameter(t.query, gl.QUERY_RESULT_AVAILABLE)) return;
      if (!gl.getParameter(t.ext.GPU_DISJOINT_EXT)) {
        this.stats.gpuMs = (gl.getQueryParameter(t.query, gl.QUERY_RESULT) as number) / 1e6;
      }
      t.waiting = false;
    }
    gl.beginQuery(t.ext.TIME_ELAPSED_EXT, t.query);
    t.active = true;
  }

  private endGpuTimer(): void {
    const t = this.timer;
    if (!t || !t.active) return;
    this.gl.endQuery(t.ext.TIME_ELAPSED_EXT);
    t.active = false;
    t.waiting = true;
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
    const gl = this.gl;
    const g = this.atlas.geometry;
    slotPlace(g, slot, this.place);
    gl.activeTexture(gl.TEXTURE0 + UNIT.atlas);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlasTexture);
    gl.texSubImage3D(
      gl.TEXTURE_2D_ARRAY,
      0,
      this.place.x,
      this.place.y,
      this.place.page,
      g.cellW,
      g.cellH,
      1,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      bitmap,
    );
    this.kinds[slot] = color ? 1 : 0;
    this.kindsDirty = true;
  }

  private uploadKinds(): void {
    if (!this.kindsDirty) return;
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0 + UNIT.kinds);
    gl.bindTexture(gl.TEXTURE_2D, this.kindsTexture);
    gl.texSubImage2D(
      gl.TEXTURE_2D,
      0,
      0,
      0,
      KIND_WIDTH,
      ATLAS_SLOTS / KIND_WIDTH,
      gl.RED_INTEGER,
      gl.UNSIGNED_BYTE,
      this.kinds,
    );
    this.kindsDirty = false;
  }

  /** The atlas is empty. The decoder gives the glyphs on screen new slots and queues them. */
  private resetAtlas(): void {
    this.atlasResets++;
    this.kinds.fill(0);
    this.kindsDirty = true;
    this.decoder.raw.atlas_reset();
    this.atlas.flushPending(this.sink);
    for (let i = 0; i < this.list.length; i++) this.list[i]!.markAll();
    this.dirty = true;
  }

  private createAtlasTexture(): void {
    const gl = this.gl;
    const g = this.atlas.geometry;
    gl.deleteTexture(this.atlasTexture);
    const texture = gl.createTexture();
    if (!texture) throw new Error('the GPU gave no texture');
    this.atlasTexture = texture;
    gl.activeTexture(gl.TEXTURE0 + UNIT.atlas);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, texture);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, g.pageW, g.pageH, g.pages);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  }

  /** Creates every GPU resource of the window. It runs again when the context comes back. */
  private build(): void {
    const gl = this.gl;
    const shapes = link(gl, SHAPE_VERTEX_SOURCE, SHAPE_FRAGMENT_SOURCE);
    this.shapeProgram = shapes;
    this.shapeUniforms = {
      rect: gl.getUniformLocation(shapes, 'uRect'),
      radii: gl.getUniformLocation(shapes, 'uRadii'),
      color: gl.getUniformLocation(shapes, 'uColor'),
      canvas: gl.getUniformLocation(shapes, 'uCanvas'),
    };
    const program = link(gl, VERTEX_SOURCE, FRAGMENT_SOURCE);
    this.program = program;
    const loc = (name: string) => gl.getUniformLocation(program, name);
    this.uniforms = {
      cells: loc('uCells'),
      ids: loc('uIds'),
      styles: loc('uStyles'),
      kinds: loc('uKinds'),
      atlasTex: loc('uAtlasTex'),
      grid: loc('uGrid'),
      cell: loc('uCell'),
      origin: loc('uOrigin'),
      canvas: loc('uCanvas'),
      sel: loc('uSel'),
      selMode: loc('uSelMode'),
      selColor: loc('uSelColor'),
      cursor: loc('uCursor'),
      cursorColor: loc('uCursorColor'),
      blink: loc('uBlink'),
      atlas: loc('uAtlas'),
      lines: loc('uLines'),
    };
    gl.useProgram(program);
    gl.uniform1i(this.uniforms.cells, UNIT.cells);
    gl.uniform1i(this.uniforms.ids, UNIT.ids);
    gl.uniform1i(this.uniforms.styles, UNIT.styles);
    gl.uniform1i(this.uniforms.kinds, UNIT.kinds);
    gl.uniform1i(this.uniforms.atlasTex, UNIT.atlas);
    this.vao = gl.createVertexArray();
    // Rows of a texture upload follow each other without padding.
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    this.createAtlasTexture();
    const kinds = gl.createTexture();
    if (!kinds) throw new Error('the GPU gave no texture');
    this.kindsTexture = kinds;
    gl.activeTexture(gl.TEXTURE0 + UNIT.kinds);
    gl.bindTexture(gl.TEXTURE_2D, kinds);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.R8UI, KIND_WIDTH, this.kinds.length / KIND_WIDTH);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    this.kindsDirty = true;
    this.staticDirty = true;
    this.dirty = true;
  }

  private readonly onLost = (event: Event): void => {
    // Without this the browser does not give the context back.
    event.preventDefault();
    this.lost = true;
  };

  private readonly onRestored = (): void => {
    // Every object of the lost context is dead. Deleting or binding one is an error, so forget
    // them all and build new ones.
    this.timer = null;
    this.program = null;
    this.shapeProgram = null;
    this.vao = null;
    this.atlasTexture = null;
    this.kindsTexture = null;
    this.build();
    for (let i = 0; i < this.list.length; i++) this.list[i]!.contextLost();
    this.lost = false;
    // The decoder still has every pane. Give its glyphs slots in the new atlas and upload all.
    this.resetAtlas();
    for (let i = 0; i < this.list.length; i++) this.list[i]!.sync();
    this.refreshBlinking();
    this.events.onContextRestored?.();
  };
}
