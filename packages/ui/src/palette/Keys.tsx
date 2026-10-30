// The keys of a list over the window, in its last row: the arrows move, Enter does the action of
// the row and Escape closes the list. The palette shows it.

import { Kbd } from '../components/ui/kbd';

export function PaletteKeys(props: { action: string }) {
  return (
    <div
      className="flex flex-none gap-4 border-t border-border px-4 py-2 text-xs text-muted-foreground"
      aria-hidden="true"
    >
      <span>
        <Kbd className="mr-0.5">↑</Kbd>
        <Kbd className="mr-0.5">↓</Kbd> Move
      </span>
      <span>
        <Kbd className="mr-0.5">↵</Kbd> {props.action}
      </span>
      <span>
        <Kbd className="mr-0.5">esc</Kbd> Close
      </span>
    </div>
  );
}
