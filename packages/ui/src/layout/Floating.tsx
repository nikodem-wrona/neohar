// A floating pane over the layout. A drag of its title strip moves it, and a drag of the corner
// resizes it. Both send `pane.set_float_rect` at most once for each animation frame. The
// rectangle on the screen is the one from the projection, with one frame of latency, and the page
// predicts nothing.

import type { FloatingPane, Rect, TabValue } from '@neohar/transport';
import { onCleanup } from 'solid-js';
import { useApp } from '../app/context';
import { createFrameSender, pageFrames } from '../lib/frames';
import { movedRect, resizedRect } from './float';
import { PaneBox } from './PaneBox';

interface Gesture {
  mode: 'move' | 'resize';
  x: number;
  y: number;
  start: Rect;
  width: number;
  height: number;
}

export function Floating(props: {
  tab: TabValue;
  item: FloatingPane;
  area: () => HTMLElement | undefined;
}) {
  const { run } = useApp();
  const sender = createFrameSender<Rect>(
    (rect) => run('pane.set_float_rect', { pane: props.item.pane, rect }),
    pageFrames,
  );
  onCleanup(() => sender.cancel());
  let gesture: Gesture | null = null;

  const begin = (event: PointerEvent): void => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    const mode = target.closest('.float-resize')
      ? 'resize'
      : target.closest('.pane-title') && !target.closest('button')
        ? 'move'
        : null;
    if (!mode) return;
    event.preventDefault();
    try {
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    } catch {
      // A pointer that the browser does not track keeps sending events to the pane anyway.
    }
    const box = props.area()?.getBoundingClientRect();
    if (!box) return;
    gesture = {
      mode,
      x: event.clientX,
      y: event.clientY,
      start: { ...props.item.rect },
      width: box.width,
      height: box.height,
    };
  };

  const move = (event: PointerEvent): void => {
    if (!gesture || gesture.width <= 0 || gesture.height <= 0) return;
    const dx = (event.clientX - gesture.x) / gesture.width;
    const dy = (event.clientY - gesture.y) / gesture.height;
    sender.push(
      gesture.mode === 'move'
        ? movedRect(gesture.start, dx, dy)
        : resizedRect(gesture.start, dx, dy),
    );
  };

  const end = (): void => {
    gesture = null;
    sender.flush();
  };

  return (
    <div
      class="floating"
      data-floating={props.item.pane}
      style={{
        left: `${props.item.rect.x * 100}%`,
        top: `${props.item.rect.y * 100}%`,
        width: `${props.item.rect.w * 100}%`,
        height: `${props.item.rect.h * 100}%`,
      }}
      onPointerDown={begin}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
    >
      <PaneBox id={props.item.pane} tab={props.tab} />
      <div class="float-resize" aria-hidden="true" />
    </div>
  );
}
