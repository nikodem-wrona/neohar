// The page that the render tests and the render bench drive.
//
// `tests/render` serves this page, loads frame files written by `nh-bench gen-frames`, and calls
// the functions of `window.bench`. Each function builds its own sessions: a canvas, a decoder and
// a renderer. Nothing here is part of the app.

import { APPLIED, createDecoder, type Decoder } from '@neohar/proto-wasm';
import {
  ATLAS_SLOTS,
  createSurface,
  GlRenderer,
  type Rect,
  type RendererKind,
  type SurfaceRenderer,
} from '../src/surface';
import { type Radii, roundedDistance } from '../src/surface/geometry';
import { type FrameFile, parseFrames } from './frames';

export interface SessionOptions {
  renderer: RendererKind;
  /** The font size in CSS pixels. The renderer gets it in device pixels. */
  fontCssPx?: number;
  family?: string;
  /** Pixels between panes. */
  gap?: number;
  /** Atlas slots, to test a full atlas. */
  slots?: number;
  /** Device pixels of background around the cells of each pane. */
  pad?: number;
  /** The radius of the bottom corners of each pane, in device pixels. */
  radius?: number;
}

/** The ground of a session with round corners: gray, so that the corners show. */
const SHAPE_GROUND = 0x404040;

export interface Stat {
  mean: number;
  p50: number;
  p99: number;
  max: number;
}

export interface PerfReport {
  renderer: RendererKind;
  gpu: string | null;
  panes: number;
  grid: string;
  cell: string;
  canvas: string;
  dpr: number;
  ticks: number;
  /** `decode` and `applyDecoded` of all frames of a tick. */
  decodeApply: Stat;
  /** CPU time to copy rows to the GPU (WebGL2 only). */
  upload: Stat;
  /** CPU time to issue the draw calls (WebGL2 only). */
  drawCpu: Stat;
  /** Everything: decode, apply, upload and draw. */
  frame: Stat;
  /** GPU time of a draw, when the browser has the timer query. */
  gpu_ms: Stat | null;
  /** The average over the whole run, which does not depend on the resolution of the clock. */
  meanFrameMs: number;
  glError: number;
}

let wasmBytes: Promise<ArrayBuffer> | null = null;
function wasm(): Promise<ArrayBuffer> {
  wasmBytes ??= fetch('/proto_wasm_bg.wasm').then((r) => r.arrayBuffer());
  return wasmBytes;
}

async function fetchFrames(url: string): Promise<FrameFile> {
  return parseFrames(await (await fetch(url)).arrayBuffer());
}

function stat(values: Float64Array): Stat {
  const sorted = Float64Array.from(values).sort();
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))] ?? 0;
  let sum = 0;
  for (const v of values) sum += v;
  return { mean: sum / Math.max(values.length, 1), p50: at(0.5), p99: at(0.99), max: at(1) };
}

/** A canvas, a decoder and a renderer, with a frame file loaded into them. */
class Session {
  readonly gl: GlRenderer | null;
  file!: FrameFile;
  /** The box of the cells and the box of the background of each pane, in device pixels. */
  readonly cells: Rect[] = [];
  readonly outers: Rect[] = [];
  readonly dpr = window.devicePixelRatio;

  private constructor(
    readonly canvas: HTMLCanvasElement,
    readonly decoder: Decoder,
    readonly surface: SurfaceRenderer,
    private readonly gap: number,
    readonly pad: number,
    readonly radius: number,
  ) {
    this.gl = surface instanceof GlRenderer ? surface : null;
  }

  static async create(options: SessionOptions): Promise<Session> {
    const canvas = document.createElement('canvas');
    document.body.append(canvas);
    const slots = options.slots ?? ATLAS_SLOTS;
    const decoder = await createDecoder(await wasm(), slots);
    const dpr = window.devicePixelRatio;
    const surface = createSurface(canvas, decoder, {
      font: {
        family: options.family ?? 'Menlo, "SF Mono", "DejaVu Sans Mono", monospace',
        sizePx: Math.round((options.fontCssPx ?? 14) * dpr),
      },
      force: options.renderer,
      slots,
    });
    const pad = options.pad ?? 0;
    const radius = options.radius ?? 0;
    if (pad > 0 || radius > 0) {
      surface.setChrome({
        ground: SHAPE_GROUND,
        frame: null,
        frameColor: SHAPE_GROUND,
        frameRadius: 0,
        paneRadius: radius,
      });
    }
    return new Session(canvas, decoder, surface, options.gap ?? 4, pad, radius);
  }

  /** Loads a file and lays the panes out in a grid on the canvas. */
  async load(url: string): Promise<void> {
    this.file = await fetchFrames(url);
    this.layout();
  }

  layout(): void {
    const { panes, cols, rows } = this.file;
    const m = this.surface.metrics;
    const pad = this.pad;
    const w = cols * m.cellW + 2 * pad;
    const h = rows * m.cellH + 2 * pad;
    const across = Math.ceil(Math.sqrt(panes));
    const down = Math.ceil(panes / across);
    this.cells.length = 0;
    this.outers.length = 0;
    for (let i = 0; i < panes; i++) {
      const id = i + 1;
      const outer = {
        x: (i % across) * (w + this.gap),
        y: Math.floor(i / across) * (h + this.gap),
        w,
        h,
      };
      const inner = { x: outer.x + pad, y: outer.y + pad, w: w - 2 * pad, h: h - 2 * pad };
      this.cells.push(inner);
      this.outers.push(outer);
      this.surface.attachPane(id);
      this.surface.setRect(id, inner, outer);
    }
    const width = across * w + (across - 1) * this.gap;
    const height = down * h + (down - 1) * this.gap;
    this.canvas.style.width = `${width / this.dpr}px`;
    this.canvas.style.height = `${height / this.dpr}px`;
    this.surface.resize({ w: width, h: height });
  }

  /**
   * Decodes the frames of a display frame and hands each to the renderer. `apply` off leaves out
   * the renderer, to find out which part of the work allocates.
   */
  playTick(tick: number, apply = true): void {
    const f = this.file;
    for (let r = f.tickStart[tick]!; r < f.tickStart[tick + 1]!; r++) {
      const status = this.decoder.decode(f.frames[r]!);
      if (status !== APPLIED) throw new Error(`tick ${tick}: decode gave status ${status}`);
      if (apply) this.surface.applyDecoded(f.paneOf[r]!);
    }
  }

  destroy(): void {
    this.surface.destroy();
    this.canvas.remove();
  }
}

/** The pixels of a canvas, read in the task that drew them. */
function readPixels(source: HTMLCanvasElement, into: HTMLCanvasElement): ImageData {
  into.width = source.width;
  into.height = source.height;
  const ctx = into.getContext('2d', { willReadFrequently: true });
  if (!ctx) throw new Error('no 2D context for the readback');
  ctx.drawImage(source, 0, 0);
  return ctx.getImageData(0, 0, into.width, into.height);
}

export interface CompareReport {
  gpu: string | null;
  ticks: number;
  cells: number;
  cellsOverTolerance: number;
  maxDifference: number;
  /** The share of pixels that differ by more than the tolerance. */
  pixelsOverTolerance: number;
  worst: Array<{ tick: number; pane: number; col: number; row: number; difference: number }>;
  /** How often each renderer reset its atlas. */
  atlasResets: { webgl2: number; canvas2d: number };
  /** The first cell over the tolerance, one line per pixel row: `#` where the renderers differ. */
  firstMask: string[];
}

async function compare(
  url: string,
  tolerance: number,
  options: Omit<SessionOptions, 'renderer'>,
): Promise<CompareReport> {
  const a = await Session.create({ ...options, renderer: 'webgl2' });
  const b = await Session.create({ ...options, renderer: 'canvas2d' });
  await Promise.all([a.load(url), b.load(url)]);
  const m = a.surface.metrics;
  const file = a.file;
  const readA = document.createElement('canvas');
  const readB = document.createElement('canvas');
  const report: CompareReport = {
    gpu: a.surface.info.gpu,
    ticks: file.ticks,
    cells: 0,
    cellsOverTolerance: 0,
    maxDifference: 0,
    pixelsOverTolerance: 0,
    worst: [],
    atlasResets: { webgl2: 0, canvas2d: 0 },
    firstMask: [],
  };
  let pixels = 0;
  let pixelsOver = 0;
  for (let tick = 0; tick < file.ticks; tick++) {
    a.playTick(tick);
    b.playTick(tick);
    a.surface.draw(0);
    b.surface.draw(0);
    const pa = readPixels(a.canvas, readA);
    const pb = readPixels(b.canvas, readB);
    for (let pane = 0; pane < file.panes; pane++) {
      const originX = a.cells[pane]!.x;
      const originY = a.cells[pane]!.y;
      for (let row = 0; row < file.rows; row++) {
        for (let col = 0; col < file.cols; col++) {
          let worst = 0;
          for (let y = 0; y < m.cellH; y++) {
            let at = ((originY + row * m.cellH + y) * pa.width + originX + col * m.cellW) * 4;
            for (let x = 0; x < m.cellW; x++, at += 4) {
              const d = Math.max(
                Math.abs(pa.data[at]! - pb.data[at]!),
                Math.abs(pa.data[at + 1]! - pb.data[at + 1]!),
                Math.abs(pa.data[at + 2]! - pb.data[at + 2]!),
              );
              if (d > worst) worst = d;
              if (d > tolerance) pixelsOver++;
              pixels++;
            }
          }
          report.cells++;
          if (worst > tolerance) {
            report.cellsOverTolerance++;
            if (report.worst.length < 8) {
              report.worst.push({ tick, pane: pane + 1, col, row, difference: worst });
            }
            if (report.firstMask.length === 0) {
              for (let y = 0; y < m.cellH; y++) {
                let line = '';
                let at = ((originY + row * m.cellH + y) * pa.width + originX + col * m.cellW) * 4;
                for (let x = 0; x < m.cellW; x++, at += 4) {
                  const d = Math.max(
                    Math.abs(pa.data[at]! - pb.data[at]!),
                    Math.abs(pa.data[at + 1]! - pb.data[at + 1]!),
                    Math.abs(pa.data[at + 2]! - pb.data[at + 2]!),
                  );
                  line +=
                    d > tolerance
                      ? '#'
                      : pa.data[at]! + pa.data[at + 1]! + pa.data[at + 2]! > 60
                        ? 'o'
                        : '.';
                }
                report.firstMask.push(line);
              }
            }
          }
          if (worst > report.maxDifference) report.maxDifference = worst;
        }
      }
    }
  }
  report.pixelsOverTolerance = pixelsOver / Math.max(pixels, 1);
  report.atlasResets = { webgl2: a.surface.atlasResets, canvas2d: b.surface.atlasResets };
  a.destroy();
  b.destroy();
  return report;
}

export interface ShapeReport {
  /** Pixels of the background boxes that differ by more than the tolerance. */
  pixelsOver: number;
  /** Of those, the ones more than one pixel from a round edge. Antialiasing explains the rest. */
  awayFromEdges: number;
  /** The colour of a corner pixel outside the curve, and of one inside, in the WebGL2 renderer. */
  outside: number[];
  inside: number[];
}

/**
 * Draws the first tick in both renderers with padding and round corners, and compares
 * the pixels of every background box. The renderers may differ only on the round edges.
 */
async function compareShapes(
  url: string,
  tolerance: number,
  options: Omit<SessionOptions, 'renderer'>,
): Promise<ShapeReport> {
  const a = await Session.create({ ...options, renderer: 'webgl2' });
  const b = await Session.create({ ...options, renderer: 'canvas2d' });
  await Promise.all([a.load(url), b.load(url)]);
  a.playTick(0);
  b.playTick(0);
  a.surface.draw(0);
  b.surface.draw(0);
  const pa = readPixels(a.canvas, document.createElement('canvas'));
  const pb = readPixels(b.canvas, document.createElement('canvas'));
  const r = a.radius;
  const radii: Radii = [0, 0, r, r];
  const report: ShapeReport = { pixelsOver: 0, awayFromEdges: 0, outside: [], inside: [] };
  for (const outer of a.outers) {
    for (let y = outer.y; y < outer.y + outer.h; y++) {
      for (let x = outer.x; x < outer.x + outer.w; x++) {
        const at = (y * pa.width + x) * 4;
        const d = Math.max(
          Math.abs(pa.data[at]! - pb.data[at]!),
          Math.abs(pa.data[at + 1]! - pb.data[at + 1]!),
          Math.abs(pa.data[at + 2]! - pb.data[at + 2]!),
        );
        if (d <= tolerance) continue;
        report.pixelsOver++;
        if (Math.abs(roundedDistance(x + 0.5, y + 0.5, outer, radii)) > 1) report.awayFromEdges++;
      }
    }
  }
  const first = a.outers[0]!;
  const pixel = (x: number, y: number) => {
    const at = (y * pa.width + x) * 4;
    return [pa.data[at]!, pa.data[at + 1]!, pa.data[at + 2]!];
  };
  report.outside = pixel(first.x + first.w - 1, first.y + first.h - 1);
  report.inside = pixel(first.x + first.w - 1 - r, first.y + first.h - 1 - r);
  a.destroy();
  b.destroy();
  return report;
}

async function perf(
  url: string,
  options: SessionOptions,
  warm: number,
  count: number,
): Promise<PerfReport> {
  const s = await Session.create(options);
  await s.load(url);
  const file = s.file;
  if (warm + count > file.ticks) throw new Error('the file has too few frames');
  if (s.gl) s.gl.timing = true;
  const decodeApply = new Float64Array(count);
  const upload = new Float64Array(count);
  const drawCpu = new Float64Array(count);
  const frame = new Float64Array(count);
  const gpu: number[] = [];
  for (let tick = 0; tick < warm + count; tick++) {
    const t0 = performance.now();
    s.playTick(tick);
    const t1 = performance.now();
    s.surface.draw(tick * 16.7);
    const t2 = performance.now();
    if (tick >= warm) {
      const i = tick - warm;
      decodeApply[i] = t1 - t0;
      frame[i] = t2 - t0;
      if (s.gl) {
        upload[i] = s.gl.stats.uploadMs;
        drawCpu[i] = s.gl.stats.drawMs;
        if (s.gl.stats.gpuMs >= 0) gpu.push(s.gl.stats.gpuMs);
      }
    }
    // Let the GPU finish between frames, so that a frame does not wait for the one before it.
    if (tick % 4 === 3) await new Promise((r) => requestAnimationFrame(r));
  }
  const glError = s.gl ? s.gl.glError() : 0;
  const m = s.surface.metrics;
  const report: PerfReport = {
    renderer: s.surface.info.kind,
    gpu: s.surface.info.gpu,
    panes: file.panes,
    grid: `${file.cols}x${file.rows}`,
    cell: `${m.cellW}x${m.cellH}`,
    canvas: `${s.canvas.width}x${s.canvas.height}`,
    dpr: s.dpr,
    ticks: count,
    decodeApply: stat(decodeApply),
    upload: stat(upload),
    drawCpu: stat(drawCpu),
    frame: stat(frame),
    gpu_ms: gpu.length > 0 ? stat(Float64Array.from(gpu)) : null,
    meanFrameMs: stat(frame).mean,
    glError,
  };
  s.destroy();
  return report;
}

/** A session that stays open, for the checks that wrap their own measurement around it. */
const sessions = new Map<number, Session>();
let nextSession = 1;

async function open(url: string, options: SessionOptions): Promise<number> {
  const s = await Session.create(options);
  await s.load(url);
  const id = nextSession++;
  sessions.set(id, s);
  return id;
}

function get(id: number): Session {
  const s = sessions.get(id);
  if (!s) throw new Error(`no session ${id}`);
  return s;
}

/** Where the waste of the control loop goes, so that it is not optimized away. */
let sink: Uint8Array | null = null;

/** The result of the last `loop`, kept apart so that returning it does not run during the loop. */
let loopResult: { cadence: Float64Array; work: Float64Array; drawn: number } | null = null;

/**
 * Plays `count` display frames from `from`, one for each `requestAnimationFrame`, and records the
 * time of each callback and the work in it. Nothing in the loop allocates.
 */
function loop(
  id: number,
  from: number,
  count: number,
  measure = true,
  stage = 3,
  garbage = 0,
): Promise<void> {
  const s = get(id);
  const cadence = new Float64Array(count);
  const work = new Float64Array(count);
  let drawn = 0;
  let i = 0;
  return new Promise((resolve) => {
    // Marks in the trace of the browser, so that a test can tell what ran inside the loop.
    console.timeStamp('nh-loop-start');
    const step = (now: number) => {
      // `measure` off leaves out the clock, which boxes a number on the heap for each read.
      const t0 = measure ? performance.now() : 0;
      // How much of the work runs: 0 nothing (the loop of the test and nothing else), 1 decode,
      // 2 decode and apply, 3 everything.
      if (stage > 0) s.playTick(from + i, stage > 1);
      if (stage > 2 && s.surface.draw(now)) drawn++;
      // A control for the test of the collector: `garbage` bytes of waste in each frame.
      if (garbage > 0) sink = new Uint8Array(garbage);
      if (measure) work[i] = performance.now() - t0;
      cadence[i] = now;
      if (++i < count) {
        requestAnimationFrame(step);
      } else {
        console.timeStamp('nh-loop-end');
        loopResult = { cadence, work, drawn };
        resolve();
      }
    };
    requestAnimationFrame(step);
  });
}

function loopReport() {
  if (!loopResult) throw new Error('no loop has run');
  const { cadence, work, drawn } = loopResult;
  const deltas = new Float64Array(Math.max(cadence.length - 1, 0));
  for (let i = 1; i < cadence.length; i++) deltas[i - 1] = cadence[i]! - cadence[i - 1]!;
  const interval = deltas.length > 0 ? stat(deltas).p50 : 0;
  let missed = 0;
  const missedAt: number[] = [];
  for (let i = 0; i < deltas.length; i++) {
    if (deltas[i]! > interval * 1.5) {
      missed++;
      if (missedAt.length < 8) missedAt.push(i + 1);
    }
  }
  return { frames: cadence.length, drawn, interval, missed, missedAt, work: stat(work) };
}

function nextEvent(target: EventTarget, type: string): Promise<void> {
  return new Promise((resolve) => target.addEventListener(type, () => resolve(), { once: true }));
}

async function contextLoss(url: string, options: SessionOptions, tick: number) {
  const id = await open(url, options);
  const s = get(id);
  if (!s.gl) throw new Error('the session has no WebGL2 renderer');
  for (let t = 0; t <= tick; t++) s.playTick(t);
  s.surface.draw(0);
  const readback = document.createElement('canvas');
  const before = readPixels(s.canvas, readback);
  const lost = nextEvent(s.canvas, 'webglcontextlost');
  const restoredEvent = nextEvent(s.canvas, 'webglcontextrestored');
  const phases: Record<string, number> = { beforeLoss: s.gl.glError() };
  const control = s.gl.loseContextForTest();
  await lost;
  // The browser allows `restoreContext` only after the lost event has been dispatched, and the
  // continuation of an `await` runs inside the dispatch. Let the event finish first.
  await new Promise((resolve) => setTimeout(resolve, 0));
  const drewWhileLost = s.surface.draw(1);
  // A frame that arrives while the context is lost is kept by the decoder, not by the GPU.
  const next = Math.min(tick + 1, s.file.ticks - 1);
  if (next > tick) s.playTick(next);
  phases.whileLost = s.gl.glError();
  control.restore();
  await restoredEvent;
  phases.afterRestore = s.gl.glError();
  s.surface.draw(2);
  phases.afterDraw = s.gl.glError();
  const after = readPixels(s.canvas, readback);
  // Draw the same frame again to compare with a renderer that never lost its context.
  const fresh = await open(url, options);
  const f = get(fresh);
  for (let t = 0; t <= next; t++) f.playTick(t);
  f.surface.draw(2);
  const reference = readPixels(f.canvas, document.createElement('canvas'));
  let different = 0;
  for (let i = 0; i < after.data.length; i += 4) {
    if (
      Math.abs(after.data[i]! - reference.data[i]!) > 2 ||
      Math.abs(after.data[i + 1]! - reference.data[i + 1]!) > 2 ||
      Math.abs(after.data[i + 2]! - reference.data[i + 2]!) > 2
    ) {
      different++;
    }
  }
  let blank = true;
  for (let i = 0; i < before.data.length; i += 4) {
    if (before.data[i] !== before.data[0]) {
      blank = false;
      break;
    }
  }
  const report = {
    drewWhileLost,
    beforeWasBlank: blank,
    differentPixelsFromFresh: different,
    pixels: after.width * after.height,
    glError: s.gl.glError(),
    phases,
  };
  get(id).destroy();
  get(fresh).destroy();
  sessions.delete(id);
  sessions.delete(fresh);
  return report;
}

/** Plays the whole file and returns whether the atlas had to reset, and a few facts. */
async function atlasFull(url: string, options: SessionOptions) {
  const id = await open(url, options);
  const s = get(id);
  for (let t = 0; t < s.file.ticks; t++) {
    s.playTick(t);
    s.surface.draw(t * 16.7);
  }
  const glError = s.gl ? s.gl.glError() : 0;
  const full = s.decoder.raw.atlas_full();
  s.destroy();
  sessions.delete(id);
  return { full, glError };
}

/**
 * Lays a surface of glass over the top left of the canvas, as the palette lies over the panes:
 * the fill and the filter of `--glass` and `--glass-filter` in `styles/tokens.css`. The
 * compositor then blurs the canvas under it in every frame that the canvas paints.
 */
function glass(on: boolean): void {
  document.querySelector('.bench-glass')?.remove();
  if (!on) return;
  const pane = document.createElement('div');
  pane.className = 'bench-glass';
  Object.assign(pane.style, {
    position: 'fixed',
    left: '40px',
    top: '40px',
    width: '720px',
    height: '480px',
    borderRadius: '14px',
    background: 'rgb(20 20 20 / 0.87)',
    backdropFilter: 'blur(24px) saturate(1.6)',
    webkitBackdropFilter: 'blur(24px) saturate(1.6)',
  });
  document.body.append(pane);
}

const bench = {
  slots: ATLAS_SLOTS,
  glass,
  /** The size of the last block of waste of a control loop. */
  wasted: () => sink?.length ?? 0,
  compare,
  compareShapes,
  perf,
  open,
  loop,
  loopReport,
  contextLoss,
  atlasFull,
  /** The canvas of a session as a PNG, for a look by eye. */
  snapshot(id: number, tick: number): string {
    const s = get(id);
    for (let t = 0; t <= tick; t++) s.playTick(t);
    s.surface.draw(0);
    const out = document.createElement('canvas');
    readPixels(s.canvas, out);
    return out.toDataURL('image/png');
  },
  info(id: number) {
    const s = get(id);
    return {
      ...s.surface.info,
      metrics: s.surface.metrics,
      dpr: s.dpr,
      canvas: [s.canvas.width, s.canvas.height],
    };
  },
  close(id: number) {
    get(id).destroy();
    sessions.delete(id);
  },
};

declare global {
  interface Window {
    bench: typeof bench;
  }
}
window.bench = bench;
