// The list of every agent, behind the `+N` cell of the board. It is a popover: it lies in
// the top layer over the panes, so no pane resizes, and Escape or a click outside closes it. Each
// row is a strip of one line, and a click on it does what a click on a strip does. The page puts
// the list under the cell, at the place that it measures when the list opens.

import type { AgentValue } from '@neohar/transport';
import { useCallback, useEffect, useRef } from 'react';
import { Strip } from './Strip';

export const BOARD_LIST = 'board-list';

export function BoardList(props: {
  runs: AgentValue[];
  work: (run: AgentValue) => string;
  tooltip: (run: AgentValue) => string;
  focused: (run: AgentValue) => boolean;
  onShow: (pane: string) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  const { onShow } = props;

  // The list opens under the `+N` cell, with its right edge at the right edge of the cell.
  useEffect(() => {
    const element = list.current;
    if (!element) return;
    const place = (event: Event): void => {
      if ((event as ToggleEvent).newState !== 'open') return;
      const cell = document.querySelector(`[popovertarget="${BOARD_LIST}"]`);
      if (!cell) return;
      const box = cell.getBoundingClientRect();
      element.style.setProperty('--list-top', `${box.bottom + 1}px`);
      element.style.setProperty('--list-right', `${Math.max(0, window.innerWidth - box.right)}px`);
    };
    element.addEventListener('beforetoggle', place);
    return () => element.removeEventListener('beforetoggle', place);
  }, []);

  const showRow = useCallback(
    (pane: string): void => {
      list.current?.hidePopover();
      onShow(pane);
    },
    [onShow],
  );

  return (
    <div
      id={BOARD_LIST}
      className="board-list"
      popover="auto"
      role="dialog"
      aria-label="Every agent"
      ref={list}
    >
      <ul aria-label="Every agent">
        {props.runs.map((run) => (
          <li key={run.id}>
            <Strip
              run={run}
              work={props.work(run)}
              tooltip={props.tooltip(run)}
              focused={props.focused(run)}
              form="one"
              onShow={showRow}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
