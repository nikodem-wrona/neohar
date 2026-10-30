// The scrollbar of a pane: a thumb that shows which part of the history the viewport has, and
// that moves the viewport when it is dragged. A click on the track above or below the thumb moves
// by a page. The page only turns positions into lines. The core scrolls (`pane.scroll`), and the
// frames that follow move the thumb.

import type { PaneId } from '@neohar/transport';
import { type MouseEvent, type PointerEvent, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { createFrameSender, pageFrames } from '../lib/frames';
import { useLatest } from '../lib/hooks';
import { lineAt, type Thumb, thumbFor } from './thumb';

/** The history that the last frame had: the first line, all lines, and the rows of the grid. */
interface History {
  thumb: Thumb | null;
  top: number;
  total: number;
  rows: number;
}

const EMPTY: History = { thumb: null, top: 0, total: 0, rows: 0 };

function sameHistory(a: History, b: History): boolean {
  return (
    a.top === b.top &&
    a.total === b.total &&
    a.rows === b.rows &&
    a.thumb?.offset === b.thumb?.offset &&
    a.thumb?.size === b.thumb?.size
  );
}

export function Scrollbar(props: { id: PaneId; renderId: number | undefined }) {
  const { run, render } = useApp();
  const [history, setHistory] = useState<History>(EMPTY);
  const { thumb, top, total, rows } = history;
  const track = useRef<HTMLDivElement>(null);
  const live = useLatest({ id: props.id, history });

  useEffect(() => {
    const id = props.renderId;
    if (render === undefined || id === undefined) return;
    const update = (): void => {
      const element = track.current;
      if (!element) return;
      const raw = render.decoder.raw;
      const top = raw.viewport_top(id);
      const total = raw.viewport_total(id);
      const rows = raw.rows(id);
      const next = { thumb: thumbFor(top, total, rows, element.clientHeight), top, total, rows };
      setHistory((before) => (sameHistory(before, next) ? before : next));
    };
    update();
    return render.watch(id, update);
  }, [render, props.renderId]);

  // A drag sends the line under the thumb, at most once for each animation frame.
  const [sender] = useState(() =>
    createFrameSender<number>(
      (line) => run('pane.scroll', { pane: live.current.id, top_line: line }),
      pageFrames,
    ),
  );
  useEffect(() => () => sender.cancel(), [sender]);
  const grab = useRef<{ pointerY: number; offset: number; size: number } | null>(null);

  const onThumbDown = (e: PointerEvent<HTMLDivElement>): void => {
    const current = thumb;
    if (e.button !== 0 || !current) return;
    e.preventDefault();
    e.stopPropagation();
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      // A pointer that the browser does not track keeps sending events to the thumb anyway.
    }
    grab.current = { pointerY: e.clientY, offset: current.offset, size: current.size };
  };
  const onThumbMove = (e: PointerEvent): void => {
    const held = grab.current;
    if (!held || !track.current) return;
    const offset = held.offset + (e.clientY - held.pointerY);
    sender.push(lineAt(offset, total, rows, track.current.clientHeight, held.size));
  };
  const onThumbUp = (): void => {
    grab.current = null;
    sender.flush();
  };
  const onTrackDown = (e: MouseEvent): void => {
    const current = thumb;
    const element = track.current;
    if (e.button !== 0 || !current || !element || e.target !== element) return;
    e.preventDefault();
    e.stopPropagation();
    const y = e.clientY - element.getBoundingClientRect().top;
    // A page up or down, in lines. The core clamps to the history.
    const delta = y < current.offset ? -rows : rows;
    run('pane.scroll', { pane: props.id, delta });
  };

  return (
    <div
      className={thumb !== null ? 'scrollbar active' : 'scrollbar'}
      ref={track}
      onMouseDown={onTrackDown}
    >
      <div
        className="scrollbar-thumb"
        role="scrollbar"
        aria-orientation="vertical"
        aria-controls={props.id}
        aria-valuemin={0}
        aria-valuemax={Math.max(total - rows, 0)}
        aria-valuenow={top}
        style={{
          display: thumb ? 'block' : 'none',
          top: `${thumb?.offset ?? 0}px`,
          height: `${thumb?.size ?? 0}px`,
        }}
        onPointerDown={onThumbDown}
        onPointerMove={onThumbMove}
        onPointerUp={onThumbUp}
        onPointerCancel={onThumbUp}
      />
    </div>
  );
}
