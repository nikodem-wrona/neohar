// Small helpers for 2D canvases that work in a window and in a worker.

/** A canvas the surface draws glyphs on. */
export type AnyCanvas = OffscreenCanvas | HTMLCanvasElement;
/** The 2D context of such a canvas. */
export type Ctx2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/** A canvas of the given size. It prefers `OffscreenCanvas`, which needs no document. */
export function makeCanvas(w: number, h: number): AnyCanvas {
  if (typeof OffscreenCanvas !== 'undefined') return new OffscreenCanvas(w, h);
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  return canvas;
}

/**
 * The 2D context of a canvas. `readback` says that the code reads pixels often, which keeps the
 * canvas in memory instead of on the GPU.
 */
export function context2d(canvas: AnyCanvas, readback = false): Ctx2D {
  const settings = { willReadFrequently: readback, alpha: true };
  const ctx = (canvas as HTMLCanvasElement).getContext('2d', settings) as Ctx2D | null;
  if (!ctx) throw new Error('the browser gives no 2D canvas context');
  return ctx;
}
