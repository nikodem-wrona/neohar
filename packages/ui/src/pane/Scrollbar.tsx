// The scrollbar of a pane: a thumb that shows which part of the history the viewport has, and
// that moves the viewport when it is dragged. A click on the track above or below the thumb moves
// by a page. The page only turns positions into lines. The core scrolls (`pane.scroll`), and the
// frames that follow move the thumb.

import type { PaneId } from '@neohar/transport';
import { createEffect, createSignal, onCleanup } from 'solid-js';
import { useApp } from '../app/context';
import { createFrameSender, pageFrames } from '../lib/frames';
import { lineAt, type Thumb, thumbFor } from './thumb';

export function Scrollbar(props: { id: PaneId; renderId: number | undefined }) {
  const { run, render } = useApp();
  const [thumb, setThumb] = createSignal<Thumb | null>(null);
  let track: HTMLDivElement | undefined;
  /** The history that the last frame had: the first line, all lines, and the rows of the grid. */
  let top = 0;
  let total = 0;
  let rows = 0;

  const update = (): void => {
    const id = props.renderId;
    if (render === undefined || id === undefined || !track) return;
    const raw = render.decoder.raw;
    top = raw.viewport_top(id);
    total = raw.viewport_total(id);
    rows = raw.rows(id);
    setThumb(thumbFor(top, total, rows, track.clientHeight));
  };
  createEffect(() => {
    const id = props.renderId;
    if (render === undefined || id === undefined) return;
    update();
    onCleanup(render.watch(id, update));
  });

  // A drag sends the line under the thumb, at most once for each animation frame.
  const sender = createFrameSender<number>(
    (line) => run('pane.scroll', { pane: props.id, top_line: line }),
    pageFrames,
  );
  onCleanup(() => sender.cancel());
  let grab: { pointerY: number; offset: number; size: number } | null = null;

  const onThumbDown = (e: PointerEvent): void => {
    const current = thumb();
    if (e.button !== 0 || !current) return;
    e.preventDefault();
    e.stopPropagation();
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      // A pointer that the browser does not track keeps sending events to the thumb anyway.
    }
    grab = { pointerY: e.clientY, offset: current.offset, size: current.size };
  };
  const onThumbMove = (e: PointerEvent): void => {
    if (!grab || !track) return;
    const offset = grab.offset + (e.clientY - grab.pointerY);
    sender.push(lineAt(offset, total, rows, track.clientHeight, grab.size));
  };
  const onThumbUp = (): void => {
    grab = null;
    sender.flush();
  };
  const onTrackDown = (e: MouseEvent): void => {
    const current = thumb();
    if (e.button !== 0 || !current || !track || e.target !== track) return;
    e.preventDefault();
    e.stopPropagation();
    const y = e.clientY - track.getBoundingClientRect().top;
    // A page up or down, in lines. The core clamps to the history.
    const delta = y < current.offset ? -rows : rows;
    run('pane.scroll', { pane: props.id, delta });
  };

  return (
    <div
      class="scrollbar"
      classList={{ active: thumb() !== null }}
      ref={(element) => (track = element)}
      onMouseDown={onTrackDown}
    >
      <div
        class="scrollbar-thumb"
        role="scrollbar"
        aria-orientation="vertical"
        aria-controls={props.id}
        aria-valuemin={0}
        aria-valuemax={Math.max(total - rows, 0)}
        aria-valuenow={top}
        style={{
          display: thumb() ? 'block' : 'none',
          top: `${thumb()?.offset ?? 0}px`,
          height: `${thumb()?.size ?? 0}px`,
        }}
        onPointerDown={onThumbDown}
        onPointerMove={onThumbMove}
        onPointerUp={onThumbUp}
        onPointerCancel={onThumbUp}
      />
    </div>
  );
}
