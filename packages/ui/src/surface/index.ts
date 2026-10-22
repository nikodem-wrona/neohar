// The surface of a window: the renderer that paints its panes.

import { type Canvas2dOptions, Canvas2dRenderer } from './canvas2d/renderer';
import { GlRenderer } from './gl/renderer';
import { chooseRenderer, probeWebGL } from './select';
import type { Decoder, FontSpec, RendererKind, SurfaceEvents, SurfaceRenderer } from './types';

export { GlRenderer } from './gl/renderer';
export { ATLAS_SLOTS } from './geometry';
export { chooseRenderer, isSoftwareRenderer, probeWebGL } from './select';
export { PLAIN_CHROME } from './types';
export type {
  CellMetrics,
  Chrome,
  FontSpec,
  Rect,
  RendererInfo,
  RendererKind,
  SelectionLines,
  Size,
  SurfaceEvents,
  SurfaceRenderer,
} from './types';

export interface SurfaceOptions {
  font: FontSpec;
  events?: SurfaceEvents;
  /** Picks a renderer by hand, for a setting or a test. */
  force?: RendererKind;
  /** The slots of the atlas, when the decoder was not made with `ATLAS_SLOTS` (tests). */
  slots?: number;
}

/**
 * The renderer for a window: WebGL2 when the browser has a GPU for it, else Canvas2D. The
 * decoder must have been made with `ATLAS_SLOTS` slots.
 */
export function createSurface(
  canvas: HTMLCanvasElement,
  decoder: Decoder,
  options: SurfaceOptions,
): SurfaceRenderer {
  const info = chooseRenderer(probeWebGL(), options.force);
  const common: Canvas2dOptions = { font: options.font, gpu: info.gpu };
  if (options.events) common.events = options.events;
  if (options.slots !== undefined) common.slots = options.slots;
  return info.kind === 'webgl2'
    ? new GlRenderer(canvas, decoder, common)
    : new Canvas2dRenderer(canvas, decoder, common);
}
