// A floating pane over the layout. A drag of its title strip moves it, and a drag of the corner
// resizes it. Both send `pane.set_float_rect` at most once for each animation frame. The
// rectangle on the screen is the one from the projection, with one frame of latency, and the page
// predicts nothing.

import type { FloatingPane, Rect, TabValue } from '@neohar/transport';
import { type PointerEvent, type RefObject, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { createFrameSender, pageFrames } from '../lib/frames';
import { useLatest } from '../lib/hooks';
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
  area: RefObject<HTMLElement | null>;
}) {
  const { run } = useApp();
  const pane = useLatest(props.item.pane);
  const [sender] = useState(() =>
    createFrameSender<Rect>(
      (rect) => run('pane.set_float_rect', { pane: pane.current, rect }),
      pageFrames,
    ),
  );
  useEffect(() => () => sender.cancel(), [sender]);
  const gesture = useRef<Gesture | null>(null);

  const begin = (event: PointerEvent<HTMLElement>): void => {
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
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that the browser does not track keeps sending events to the pane anyway.
    }
    const box = props.area.current?.getBoundingClientRect();
    if (!box) return;
    gesture.current = {
      mode,
      x: event.clientX,
      y: event.clientY,
      start: { ...props.item.rect },
      width: box.width,
      height: box.height,
    };
  };

  const move = (event: PointerEvent): void => {
    const held = gesture.current;
    if (!held || held.width <= 0 || held.height <= 0) return;
    const dx = (event.clientX - held.x) / held.width;
    const dy = (event.clientY - held.y) / held.height;
    sender.push(
      held.mode === 'move' ? movedRect(held.start, dx, dy) : resizedRect(held.start, dx, dy),
    );
  };

  const end = (): void => {
    gesture.current = null;
    sender.flush();
  };

  return (
    <div
      className="floating"
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
      <div className="float-resize" aria-hidden="true" />
    </div>
  );
}
