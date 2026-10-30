// The list of every agent, behind the `+N` cell of the board. It is the content of a Popover of
// shadcn/ui: it lies in a layer over the panes, so no pane resizes, and Escape or a click outside
// closes it. Each row is a strip of one line, and a click on it does what a click on a strip does.
// The list opens under the cell, with its right edge at the right edge of the cell.

import type { AgentValue } from '@neohar/transport';
import { PopoverContent } from '../components/ui/popover';
import { Strip } from './Strip';

export function BoardList(props: {
  runs: AgentValue[];
  work: (run: AgentValue) => string;
  tooltip: (run: AgentValue) => string;
  focused: (run: AgentValue) => boolean;
  /** Shows the pane of a row. The list closes. */
  onShow: (pane: string) => void;
}) {
  return (
    <PopoverContent
      align="end"
      sideOffset={1}
      aria-label="Every agent"
      // `board-list` sizes the strips of its rows (`styles/board.css`).
      className="board-list max-h-[60vh] w-[44ch] overflow-y-auto rounded-(--r-md) p-1"
    >
      <ul className="m-0 list-none p-0" aria-label="Every agent">
        {props.runs.map((run) => (
          <li key={run.id}>
            <Strip
              run={run}
              work={props.work(run)}
              tooltip={props.tooltip(run)}
              focused={props.focused(run)}
              form="one"
              onShow={props.onShow}
            />
          </li>
        ))}
      </ul>
    </PopoverContent>
  );
}
