// Which renderer paints: WebGL2 when there is a GPU, else Canvas2D.

import type { RendererInfo, RendererKind } from './types';

/** Names that WebGL implementations on the CPU give to themselves. */
const SOFTWARE = /swiftshader|llvmpipe|softpipe|lavapipe|software|basic render|offscreen/i;

/** Whether the name of a GPU (`UNMASKED_RENDERER_WEBGL`) is a renderer on the CPU. */
export function isSoftwareRenderer(name: string | null): boolean {
  return name !== null && SOFTWARE.test(name);
}

/** What a probe of the browser found. */
export interface Probe {
  /** WebGL2 exists and it is not a slow fallback of the browser. */
  webgl2: boolean;
  gpu: string | null;
}

/**
 * The renderer for a probe. `force` picks one by hand (a setting, or a test). Without it, WebGL2
 * is used unless it is absent or runs on the CPU.
 */
export function chooseRenderer(probe: Probe, force?: RendererKind): RendererInfo {
  const kind: RendererKind =
    force ?? (probe.webgl2 && !isSoftwareRenderer(probe.gpu) ? 'webgl2' : 'canvas2d');
  return { kind, gpu: probe.gpu };
}

/**
 * Asks the browser what WebGL it has, with a canvas that it throws away. A canvas keeps the type
 * of its first context, so the probe must not use the canvas of the window.
 */
export function probeWebGL(): Probe {
  const canvas = document.createElement('canvas');
  // `failIfMajorPerformanceCaveat` makes the browser refuse a software context.
  const gl =
    canvas.getContext('webgl2', { failIfMajorPerformanceCaveat: true }) ??
    canvas.getContext('webgl2');
  if (!gl) return { webgl2: false, gpu: null };
  const debug = gl.getExtension('WEBGL_debug_renderer_info');
  const gpu = debug ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)) : null;
  const strict = canvas.getContext('webgl2', { failIfMajorPerformanceCaveat: true }) !== null;
  gl.getExtension('WEBGL_lose_context')?.loseContext();
  return { webgl2: strict, gpu };
}
