// The list of every agent, behind the `+N` cell of the board. It is a popover: it lies in
// the top layer over the panes, so no pane resizes, and Escape or a click outside closes it. Each
// row is a strip of one line, and a click on it does what a click on a strip does. The page puts
// the list under the cell, at the place that it measures when the list opens.

import type { AgentValue } from '@neohar/transport';
import { For } from 'solid-js';
import { Strip } from './Strip';

export const BOARD_LIST = 'board-list';

export function BoardList(props: {
  runs: AgentValue[];
  work: (run: AgentValue) => string;
  tooltip: (run: AgentValue) => string;
  focused: (run: AgentValue) => boolean;
  onShow: (pane: string) => void;
}) {
  let list: HTMLDivElement | undefined;

  // The list opens under the `+N` cell, with its right edge at the right edge of the cell.
  const place = (event: Event): void => {
    if ((event as ToggleEvent).newState !== 'open' || !list) return;
    const cell = document.querySelector(`[popovertarget="${BOARD_LIST}"]`);
    if (!cell) return;
    const box = cell.getBoundingClientRect();
    list.style.setProperty('--list-top', `${box.bottom + 1}px`);
    list.style.setProperty('--list-right', `${Math.max(0, window.innerWidth - box.right)}px`);
  };

  return (
    <div
      id={BOARD_LIST}
      class="board-list"
      popover="auto"
      role="dialog"
      aria-label="Every agent"
      ref={(element) => {
        list = element;
        element.addEventListener('beforetoggle', place);
      }}
    >
      <ul aria-label="Every agent">
        <For each={props.runs}>
          {(run) => (
            <li>
              <Strip
                run={run}
                work={props.work(run)}
                tooltip={props.tooltip(run)}
                focused={props.focused(run)}
                form="one"
                onShow={(pane) => {
                  list?.hidePopover();
                  props.onShow(pane);
                }}
              />
            </li>
          )}
        </For>
      </ul>
    </div>
  );
}
