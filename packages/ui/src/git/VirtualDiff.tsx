// The diff of one file in the Git view. It can have 20,000 lines, so the box paints only the rows
// on screen and some more on each side: each row has the same height, and a spacer of the whole
// height keeps the scroll bar true. A row keeps its line on one line and the box scrolls to the
// side, so that every row has its height.

import type { FileDiff } from '@neohar/transport';
import { createMemo, createSignal, For, Match, onCleanup, onMount, Switch } from 'solid-js';
import { lineSegments, marker } from '../rich/lines';
import { diffRows, visibleRows } from './model';

/** The height of a row in CSS pixels, as `.git-vrow` sets it. */
const ROW = 18;
/** The rows that the box paints above and below the screen. */
const EXTRA = 40;

export function VirtualDiff(props: { file: FileDiff; label: string }) {
  let box: HTMLDivElement | undefined;
  const rows = createMemo(() => diffRows(props.file));
  const [top, setTop] = createSignal(0);
  const [height, setHeight] = createSignal(0);
  const range = createMemo(() => visibleRows(rows().length, top(), height(), ROW, EXTRA));

  onMount(() => {
    if (!box) return;
    const observer = new ResizeObserver(() => setHeight(box?.clientHeight ?? 0));
    observer.observe(box);
    setHeight(box.clientHeight);
    onCleanup(() => observer.disconnect());
  });

  return (
    <div
      class="git-diff-body diff"
      role="group"
      aria-label={props.label}
      tabIndex={0}
      ref={(element) => (box = element)}
      onScroll={(event) => setTop(event.currentTarget.scrollTop)}
    >
      <div class="git-vspace" style={{ height: `${rows().length * ROW}px` }}>
        <div class="git-vrows" style={{ transform: `translateY(${range().start * ROW}px)` }}>
          <For each={rows().slice(range().start, range().end)}>
            {(row) => (
              <Switch>
                <Match when={row.kind === 'hunk' && row}>
                  {(hunk) => <div class="diff-hunk-header git-vrow">{hunk().header}</div>}
                </Match>
                <Match when={row.kind === 'line' && row}>
                  {(shown) => {
                    const line = shown().line;
                    return (
                      <div class="diff-line git-vrow" classList={{ [`diff-${line.kind}`]: true }}>
                        <span class="diff-num" aria-hidden="true">
                          {line.old ?? ''}
                        </span>
                        <span class="diff-num" aria-hidden="true">
                          {line.new ?? ''}
                        </span>
                        <span class="diff-marker" aria-hidden="true">
                          {marker(line.kind)}
                        </span>
                        <span class="diff-text">
                          <For each={lineSegments(line)}>
                            {(run) =>
                              run.className ? (
                                <span class={run.className}>{run.text}</span>
                              ) : (
                                run.text
                              )
                            }
                          </For>
                        </span>
                      </div>
                    );
                  }}
                </Match>
              </Switch>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
