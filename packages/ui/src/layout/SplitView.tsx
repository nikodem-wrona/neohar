// A split with its dividers. A drag of a divider shows the new shares at once and sends one
// `layout.set_ratios` when the pointer is released. This is the one local prediction of the UI.
// A double click on a divider equalizes the shares. The arrow keys move a focused divider.

import type { LayoutNode, TabValue } from '@neohar/transport';
import {
  Fragment,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { useLatest } from '../lib/hooks';
import { cn } from '../lib/utils';
import { dragRatios, ratiosDiffer } from './divider';
import { flexOf } from './tree';

type Split = Extract<LayoutNode, { type: 'split' }>;

/** How far an arrow key moves a divider, as a share of the split. */
const KEY_STEP = 0.02;

/**
 * The key of a child of a split: a pane by its id, a split by its place. A pane that stays at its
 * place keeps its box, and so its surface.
 */
function childKey(node: LayoutNode, index: number): string {
  return node.type === 'pane' ? `pane:${node.pane}` : `split:${index}`;
}

export function SplitView(props: {
  tab: TabValue;
  split: Split;
  path: number[];
  /** Draws the child at the given place. The layout view gives it, because the two call each other. */
  child: (node: LayoutNode, path: number[]) => ReactNode;
}) {
  const { transport, run, report } = useApp();
  const [preview, setPreview] = useState<number[] | null>(null);
  const ratios = preview ?? props.split.ratios;
  const element = useRef<HTMLDivElement>(null);
  const drag = useRef<{ index: number; origin: number; start: number[]; room: number } | null>(
    null,
  );
  const column = props.split.axis === 'column';
  const latest = useLatest(props.split.ratios);

  // The projection has the final shares: the preview has done its work.
  const shares = props.split.ratios.join(',');
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    setPreview(null);
  }, [shares]);

  const send = (next: number[]): void => {
    setPreview(next);
    transport
      .call('layout.set_ratios', { tab: props.tab.id, path: props.path, ratios: next })
      .then(() => {
        if (!ratiosDiffer(next, latest.current)) setPreview(null);
      })
      .catch((failure: unknown) => {
        setPreview(null);
        report(failure);
      });
  };

  const begin = (index: number, event: PointerEvent<HTMLElement>): void => {
    if (event.button !== 0) return;
    const handle = event.currentTarget;
    event.preventDefault();
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that the browser does not track keeps sending events to the divider anyway.
    }
    const box = element.current?.getBoundingClientRect();
    if (!box) return;
    const thickness = handle.getBoundingClientRect();
    const dividers = props.split.children.length - 1;
    drag.current = {
      index,
      origin: column ? event.clientY : event.clientX,
      start: [...props.split.ratios],
      room: column
        ? box.height - dividers * thickness.height
        : box.width - dividers * thickness.width,
    };
  };

  const move = (event: PointerEvent): void => {
    const held = drag.current;
    if (!held || held.room <= 0) return;
    const pointer = column ? event.clientY : event.clientX;
    setPreview(dragRatios(held.start, held.index, (pointer - held.origin) / held.room));
  };

  const end = (): void => {
    const finished = drag.current;
    drag.current = null;
    const next = preview;
    if (!finished || !next) return;
    if (ratiosDiffer(next, finished.start)) send(next);
    else setPreview(null);
  };

  const key = (index: number, event: KeyboardEvent): void => {
    const before = column ? 'ArrowUp' : 'ArrowLeft';
    const after = column ? 'ArrowDown' : 'ArrowRight';
    if (event.key !== before && event.key !== after) return;
    event.preventDefault();
    const next = dragRatios(props.split.ratios, index, event.key === before ? -KEY_STEP : KEY_STEP);
    if (ratiosDiffer(next, props.split.ratios)) send(next);
  };

  const equalize = (): void => run('layout.equalize', { tab: props.tab.id, path: props.path });

  return (
    <div className={cn('split', { column })} ref={element}>
      {props.split.children.map((child, index) => (
        <Fragment key={childKey(child, index)}>
          {index > 0 ? (
            <div
              className="divider"
              role="separator"
              tabIndex={0}
              aria-orientation={column ? 'horizontal' : 'vertical'}
              aria-valuenow={Math.round((ratios[index - 1] ?? 0) * 100)}
              data-divider={index - 1}
              onPointerDown={(event) => begin(index - 1, event)}
              onPointerMove={move}
              onPointerUp={end}
              onPointerCancel={end}
              onDoubleClick={equalize}
              onKeyDown={(event) => key(index - 1, event)}
            />
          ) : null}
          <div className="split-child" style={{ flex: flexOf(ratios[index]) }}>
            {props.child(child, [...props.path, index])}
          </div>
        </Fragment>
      ))}
    </div>
  );
}
