// The render path of one window. The window has one canvas and one renderer.
// The core sends frames on the `render` stream. The host gives each one to the decoder and to the
// renderer, paints in the next animation frame, and then tells the core what it painted
// (`render.ack`). The host tracks the box of each pane in the DOM, gives the renderer the
// rectangle, and tells the core the size in cells (`pane.resize`). It also gives the renderer the
// chrome that the canvas paints: the ground, the main panel that holds the panes, and the
// round corners of a pane, from the custom properties of the panel.
//
// The page holds no terminal state. The decoder and the renderer keep a copy of what the core
// sent, for the GPU.

import { APPLIED, type Decoder, frameProto, NEEDS_KEYFRAME } from '@neohar/proto-wasm';
import type { BinaryStream, Transport } from '@neohar/transport';
import { type Frames, pageFrames } from '../lib/frames';
import { realTimers, type Timers } from '../lib/timing';
import {
  type Chrome,
  createSurface,
  type FontSpec,
  PLAIN_CHROME,
  type Rect,
  type SurfaceRenderer,
} from '../surface';
import { AckBook } from './acks';
import { cssColor, cssPixels } from './colors';
import { DEFAULT_FONT_FAMILY, DEFAULT_FONT_PX } from './defaults';
import { type Cells, cellsFor, ResizeScheduler } from './resize';

export { DEFAULT_FONT_FAMILY, DEFAULT_FONT_PX } from './defaults';

export interface RenderHostOptions {
  transport: Transport;
  windowId: string;
  decoder: Decoder;
  fontFamily?: string;
  /** In CSS pixels. The renderer gets it multiplied by the pixel ratio of the display. */
  fontPx?: number;
  frames?: Frames;
  timers?: Timers;
}

/** A pane that the host paints. */
interface Attached {
  paneId: string;
  /** The box of the cells. */
  element: HTMLElement;
  /** The box of the background: the cells and the padding around them. */
  outerElement: HTMLElement;
  /** The rectangles that the renderer has, in device pixels. */
  rect: Rect | null;
  outer: Rect | null;
  /** The size that the core has: what the last `pane.resize` said, or what the projection said. */
  known: Cells | null;
  resize: ResizeScheduler<Cells>;
  /** Tells the host that the box changed its size. A change of place alone shows in the DOM. */
  observer: ResizeObserver;
}

export class RenderHost {
  readonly canvas: HTMLCanvasElement;
  private readonly panes = new Map<number, Attached>();
  private readonly watchers = new Map<number, Set<() => void>>();
  private readonly book = new AckBook();
  private readonly frames: Frames;
  private readonly timers: Timers;
  private pending: number | null = null;
  private dirty = true;
  private stream: BinaryStream | null = null;
  private observer: MutationObserver | null = null;
  private watchRatio: (() => void) | null = null;
  private ratio = window.devicePixelRatio;
  /** The main panel: its box, its fill and the radii come from it. */
  private frameElement: HTMLElement | null = null;
  private frameObserver: ResizeObserver | null = null;
  private chrome: Chrome = PLAIN_CHROME;

  private constructor(
    private readonly options: RenderHostOptions,
    readonly surface: SurfaceRenderer,
    canvas: HTMLCanvasElement,
  ) {
    this.canvas = canvas;
    this.frames = options.frames ?? pageFrames;
    this.timers = options.timers ?? realTimers;
  }

  /** Makes the canvas and the renderer of a window. */
  static create(options: RenderHostOptions): RenderHost {
    const canvas = document.createElement('canvas');
    canvas.className = 'surface';
    canvas.setAttribute('aria-hidden', 'true');
    const surface = createSurface(canvas, options.decoder, {
      font: fontFor(options, window.devicePixelRatio),
    });
    return new RenderHost(options, surface, canvas);
  }

  /** The decoder, for the getters of a pane: the cursor, the modes, the viewport, the selection. */
  get decoder(): Decoder {
    return this.options.decoder;
  }

  /** The size of a cell in CSS pixels. */
  get cell(): { w: number; h: number } {
    const dpr = window.devicePixelRatio;
    return { w: this.surface.metrics.cellW / dpr, h: this.surface.metrics.cellH / dpr };
  }

  /**
   * The first column and row of the grid of a pane that its box shows: 0 and 0, unless another
   * window has the size of the PTY and the grid is larger than the box.
   */
  crop(renderId: number): { left: number; top: number } {
    return this.surface.crop(renderId);
  }

  /** Moves the part of a larger grid that the box shows, in cells (two fingers on a phone). */
  setPan(renderId: number, left: number, top: number): void {
    this.surface.setPan(renderId, left, top);
    this.invalidate();
  }

  /** Calls `listener` after each frame of a pane that the decoder applied. */
  watch(renderId: number, listener: () => void): () => void {
    const set = this.watchers.get(renderId) ?? new Set();
    set.add(listener);
    this.watchers.set(renderId, set);
    return () => {
      set.delete(listener);
      if (set.size === 0) this.watchers.delete(renderId);
    };
  }

  /**
   * Changes the font. The cells get new metrics, every pane is measured again, and the core gets
   * the new cell size. A change of the font in the settings calls it. `fresh` says that the faces of the
   * family have just loaded: the font is measured again and the atlas is filled again,
   * also when the family and the size are the same.
   */
  setFont(font: { family?: string; px?: number }, fresh = false): void {
    const family = font.family ?? this.options.fontFamily ?? DEFAULT_FONT_FAMILY;
    const px = font.px ?? this.options.fontPx ?? DEFAULT_FONT_PX;
    if (
      !fresh &&
      family === (this.options.fontFamily ?? DEFAULT_FONT_FAMILY) &&
      px === (this.options.fontPx ?? DEFAULT_FONT_PX)
    ) {
      return;
    }
    this.options.fontFamily = family;
    this.options.fontPx = px;
    this.surface.setFont(fontFor(this.options, window.devicePixelRatio));
    this.reportMetrics();
    for (const attached of this.panes.values()) attached.rect = null;
    this.invalidate();
  }

  /** The renderer in use, for the status bar and the report to the core. */
  get renderer(): string {
    return this.surface.info.kind;
  }

  start(): void {
    this.resizeCanvas();
    this.reportMetrics();
    window.addEventListener('resize', this.onWindowResize);
    this.watchPixelRatio();
    this.stream = this.options.transport.stream(
      'render',
      { window: this.options.windowId, viewport_hz: 60, proto: frameProto() },
      (bytes) => this.onFrame(new Uint8Array(bytes)),
    );
  }

  stop(): void {
    this.stream?.close();
    this.stream = null;
    this.observer?.disconnect();
    this.frameObserver?.disconnect();
    this.watchRatio?.();
    window.removeEventListener('resize', this.onWindowResize);
    if (this.pending !== null) this.frames.cancel(this.pending);
    this.pending = null;
    for (const attached of this.panes.values()) {
      attached.resize.cancel();
      attached.observer.disconnect();
    }
    this.panes.clear();
    this.surface.destroy();
  }

  /**
   * Watches a part of the DOM for changes that move a pane: a new style, a new class, a new
   * child. The boxes of the panes are measured again before the next paint.
   */
  observe(root: HTMLElement): void {
    this.observer?.disconnect();
    this.observer = new MutationObserver(() => this.invalidate());
    this.observer.observe(root, {
      attributes: true,
      attributeFilter: ['style', 'class'],
      childList: true,
      subtree: true,
    });
  }

  /**
   * Paints the main panel of the window under the box of `element`. The panel sets
   * `--frame-fill`, `--frame-radius` and `--pane-radius`, and `--bg` is the ground around it. The
   * result stops painting the panel.
   */
  attachFrame(element: HTMLElement): () => void {
    this.frameObserver?.disconnect();
    this.frameElement = element;
    this.frameObserver = new ResizeObserver(() => this.invalidate());
    this.frameObserver.observe(element);
    this.invalidate();
    return () => {
      if (this.frameElement !== element) return;
      this.frameObserver?.disconnect();
      this.frameObserver = null;
      this.frameElement = null;
      this.invalidate();
    };
  }

  /**
   * Paints the pane with render id `renderId`: its cells into the box of `element`, and its
   * background into the box of `outer` (the box with the padding). `known` is the size in cells
   * that the core has. The result stops painting the pane.
   */
  attach(
    renderId: number,
    paneId: string,
    element: HTMLElement,
    known?: Cells,
    outer: HTMLElement = element,
  ): () => void {
    this.surface.attachPane(renderId);
    // A frame can come before the pane: the decoder has it and the renderer has not seen it.
    if (this.options.decoder.raw.cols(renderId) > 0) this.surface.applyDecoded(renderId);
    const attached: Attached = {
      paneId,
      element,
      outerElement: outer,
      rect: null,
      outer: null,
      known: known ?? null,
      resize: new ResizeScheduler<Cells>((cells) => this.sendResize(attached, cells), this.timers),
      observer: new ResizeObserver(() => this.invalidate()),
    };
    attached.observer.observe(element);
    this.panes.set(renderId, attached);
    this.invalidate();
    return () => {
      if (this.panes.get(renderId) !== attached) return;
      attached.resize.cancel();
      attached.observer.disconnect();
      this.panes.delete(renderId);
      this.book.forget(renderId);
      this.surface.detachPane(renderId);
      this.options.decoder.raw.remove_pane(renderId);
      this.invalidate();
    };
  }

  /** The boxes of the panes may have moved: measure them before the next paint. */
  invalidate(): void {
    this.dirty = true;
    this.schedule();
  }

  // Frames ------------------------------------------------------------------------------------

  private onFrame(bytes: Uint8Array, ack = true): void {
    const decoder = this.options.decoder;
    const status = decoder.decode(bytes);
    // The decoder names the pane, also of a frame that it rejects. The page parses no bytes.
    const pane = decoder.raw.pane();
    if (status === APPLIED) {
      this.surface.applyDecoded(pane);
      // A frame can come before the box of its pane. The decoder keeps it, the pane shows it
      // when it attaches, and the ack frees the slot of the frame now.
      if (ack) this.book.paint(pane, decoder.raw.rev());
      this.watchers.get(pane)?.forEach((listener) => listener());
    } else if (status === NEEDS_KEYFRAME) {
      if (ack) this.book.needKeyframe(pane);
    } else {
      console.error('the core sent bytes that are not a frame');
    }
    this.schedule();
  }

  private schedule(): void {
    if (this.pending === null) this.pending = this.frames.request(this.tick);
  }

  private readonly tick = (): void => {
    this.pending = null;
    if (this.dirty) {
      this.dirty = false;
      this.layoutPass();
    }
    this.surface.draw(performance.now());
    const acks = this.book.take();
    if (acks) {
      // An ack fails only when its stream ended, and a new stream starts with a keyframe.
      void this.options.transport
        .call('render.ack', { window: this.options.windowId, ...acks })
        .catch(() => undefined);
    }
    // A blinking cursor changes without a frame, so the host keeps painting while one blinks.
    if (this.surface.blinking) this.schedule();
  };

  // Boxes and sizes ---------------------------------------------------------------------------

  /** Gives the renderer the rectangle of each pane, and the core the size in cells. */
  private layoutPass(): void {
    const dpr = window.devicePixelRatio;
    this.chromePass(dpr);
    for (const [renderId, attached] of this.panes) {
      const rect = boxOf(attached.element, dpr);
      const outer =
        attached.outerElement === attached.element ? rect : boxOf(attached.outerElement, dpr);
      if (sameRect(attached.rect, rect) && sameRect(attached.outer, outer)) continue;
      attached.rect = rect;
      attached.outer = outer;
      this.surface.setRect(renderId, rect, outer);
      if (rect.w > 0 && rect.h > 0) {
        const cells = cellsFor(rect, this.surface.metrics);
        const known = attached.known;
        if (!known || known.cols !== cells.cols || known.rows !== cells.rows) {
          attached.resize.request(cells);
        } else {
          // The size is what the core has: a pending change that came back to it is not needed.
          attached.resize.cancel();
        }
      }
    }
  }

  /** Reads the chrome from the main panel, and gives it to the renderer when it changed. */
  private chromePass(dpr: number): void {
    const element = this.frameElement;
    const style = getComputedStyle(element ?? document.documentElement);
    const value = (name: string) => style.getPropertyValue(name);
    const chrome: Chrome = {
      ground: cssColor(value('--bg'), PLAIN_CHROME.ground),
      frame: element ? boxOf(element, dpr) : null,
      frameColor: cssColor(value('--frame-fill'), PLAIN_CHROME.frameColor),
      frameRadius: Math.round(cssPixels(value('--frame-radius')) * dpr),
      paneRadius: Math.round(cssPixels(value('--pane-radius')) * dpr),
    };
    const last = this.chrome;
    if (
      chrome.ground === last.ground &&
      chrome.frameColor === last.frameColor &&
      chrome.frameRadius === last.frameRadius &&
      chrome.paneRadius === last.paneRadius &&
      (chrome.frame === null) === (last.frame === null) &&
      (chrome.frame === null || sameRect(last.frame, chrome.frame))
    ) {
      return;
    }
    this.chrome = chrome;
    this.surface.setChrome(chrome);
  }

  private sendResize(attached: Attached, cells: Cells): void {
    const known = attached.known;
    if (known && known.cols === cells.cols && known.rows === cells.rows) return;
    attached.known = cells;
    // A pane that closed in the meantime refuses its size, and nothing waits for it.
    void this.options.transport
      .call('pane.resize', { pane: attached.paneId, window: this.options.windowId, ...cells })
      .catch(() => undefined);
  }

  private resizeCanvas(): void {
    const dpr = window.devicePixelRatio;
    const width = window.innerWidth;
    const height = window.innerHeight;
    this.canvas.style.width = `${width}px`;
    this.canvas.style.height = `${height}px`;
    this.surface.resize({ w: Math.round(width * dpr), h: Math.round(height * dpr) });
  }

  private readonly onWindowResize = (): void => {
    this.resizeCanvas();
    this.invalidate();
  };

  // The display and the font ------------------------------------------------------------------

  /** What the page measured: the size of a cell and the renderer in use. */
  private reportMetrics(): void {
    const dpr = window.devicePixelRatio;
    const metrics = this.surface.metrics;
    const { kind, gpu } = this.surface.info;
    // The core refuses the metrics only of a window that it does not know, and the shell says so.
    void this.options.transport
      .call('window.report_metrics', {
        window: this.options.windowId,
        cell_width_px: metrics.cellW / dpr,
        cell_height_px: metrics.cellH / dpr,
        device_pixel_ratio: dpr,
        renderer: kind,
        ...(gpu ? { gpu } : {}),
      })
      .catch(() => undefined);
  }

  /** A move to another display changes the pixel ratio: the font and the cells change with it. */
  private watchPixelRatio(): void {
    const query = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    const changed = (): void => {
      query.removeEventListener('change', changed);
      this.ratio = window.devicePixelRatio;
      this.surface.setFont(fontFor(this.options, this.ratio));
      this.resizeCanvas();
      this.reportMetrics();
      // The cells have another size in pixels, so each pane has another size in cells.
      for (const attached of this.panes.values()) attached.rect = null;
      this.invalidate();
      this.watchPixelRatio();
    };
    query.addEventListener('change', changed);
    this.watchRatio = () => query.removeEventListener('change', changed);
  }
}

/** The box of an element in device pixels of the canvas. */
function boxOf(element: HTMLElement, dpr: number): Rect {
  const box = element.getBoundingClientRect();
  return {
    x: Math.round(box.left * dpr),
    y: Math.round(box.top * dpr),
    w: Math.round(box.width * dpr),
    h: Math.round(box.height * dpr),
  };
}

function sameRect(a: Rect | null, b: Rect): boolean {
  return a !== null && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
}

function fontFor(options: RenderHostOptions, dpr: number): FontSpec {
  return {
    family: options.fontFamily ?? DEFAULT_FONT_FAMILY,
    sizePx: Math.round((options.fontPx ?? DEFAULT_FONT_PX) * dpr),
  };
}
