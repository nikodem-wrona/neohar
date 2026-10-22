// A split with its dividers. A drag of a divider shows the new shares at once and sends one
// `layout.set_ratios` when the pointer is released. This is the one local prediction of the UI.
// A double click on a divider equalizes the shares. The arrow keys move a focused divider.

import type { LayoutNode, TabValue } from '@neohar/transport';
import { createEffect, createSignal, For, on, Show, type JSX } from 'solid-js';
import { useApp } from '../app/context';
import { dragRatios, ratiosDiffer } from './divider';
import { flexOf } from './tree';

type Split = Extract<LayoutNode, { type: 'split' }>;

/** How far an arrow key moves a divider, as a share of the split. */
const KEY_STEP = 0.02;

export function SplitView(props: {
  tab: TabValue;
  split: Split;
  path: number[];
  /** Draws the child at the given place. The layout view gives it, because the two call each other. */
  child: (node: LayoutNode, path: number[]) => JSX.Element;
}) {
  const { transport, run, report } = useApp();
  const [preview, setPreview] = createSignal<number[] | null>(null);
  const ratios = () => preview() ?? props.split.ratios;
  let element: HTMLDivElement | undefined;
  let drag: { index: number; origin: number; start: number[]; room: number } | null = null;
  const column = () => props.split.axis === 'column';

  // The projection has the final shares: the preview has done its work.
  createEffect(
    on(
      () => props.split.ratios.join(','),
      () => setPreview(null),
      { defer: true },
    ),
  );

  const send = (next: number[]): void => {
    setPreview(next);
    transport
      .call('layout.set_ratios', { tab: props.tab.id, path: props.path, ratios: next })
      .then(() => {
        if (!ratiosDiffer(next, props.split.ratios)) setPreview(null);
      })
      .catch((failure: unknown) => {
        setPreview(null);
        report(failure);
      });
  };

  const begin = (index: number, event: PointerEvent): void => {
    if (event.button !== 0) return;
    const handle = event.currentTarget as HTMLElement;
    event.preventDefault();
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that the browser does not track keeps sending events to the divider anyway.
    }
    const box = element?.getBoundingClientRect();
    if (!box) return;
    const thickness = handle.getBoundingClientRect();
    const dividers = props.split.children.length - 1;
    drag = {
      index,
      origin: column() ? event.clientY : event.clientX,
      start: [...props.split.ratios],
      room: column()
        ? box.height - dividers * thickness.height
        : box.width - dividers * thickness.width,
    };
  };

  const move = (event: PointerEvent): void => {
    if (!drag || drag.room <= 0) return;
    const pointer = column() ? event.clientY : event.clientX;
    setPreview(dragRatios(drag.start, drag.index, (pointer - drag.origin) / drag.room));
  };

  const end = (): void => {
    const finished = drag;
    drag = null;
    const next = preview();
    if (!finished || !next) return;
    if (ratiosDiffer(next, finished.start)) send(next);
    else setPreview(null);
  };

  const key = (index: number, event: KeyboardEvent): void => {
    const before = column() ? 'ArrowUp' : 'ArrowLeft';
    const after = column() ? 'ArrowDown' : 'ArrowRight';
    if (event.key !== before && event.key !== after) return;
    event.preventDefault();
    const next = dragRatios(props.split.ratios, index, event.key === before ? -KEY_STEP : KEY_STEP);
    if (ratiosDiffer(next, props.split.ratios)) send(next);
  };

  const equalize = (): void => run('layout.equalize', { tab: props.tab.id, path: props.path });

  return (
    <div class="split" classList={{ column: column() }} ref={(el) => (element = el)}>
      <For each={props.split.children}>
        {(child, index) => (
          <>
            <Show when={index() > 0}>
              <div
                class="divider"
                role="separator"
                tabindex="0"
                aria-orientation={column() ? 'horizontal' : 'vertical'}
                aria-valuenow={Math.round((ratios()[index() - 1] ?? 0) * 100)}
                data-divider={index() - 1}
                onPointerDown={(event) => begin(index() - 1, event)}
                onPointerMove={move}
                onPointerUp={end}
                onPointerCancel={end}
                onDblClick={equalize}
                onKeyDown={(event) => key(index() - 1, event)}
              />
            </Show>
            <div class="split-child" style={{ flex: flexOf(ratios()[index()]) }}>
              {props.child(child, [...props.path, index()])}
            </div>
          </>
        )}
      </For>
    </div>
  );
}
