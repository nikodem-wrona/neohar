// The icons of the rails: line drawings of 1.5 px in the colour of the text, 16 px, inline
// SVG. A rail button has the icon and no word, so its name is the accessible
// name and the tooltip of the button.

import type { JSX } from 'solid-js';

export type RailIcon = 'workspaces' | 'inbox' | 'timeline' | 'tasks' | 'files' | 'changes';

const PATHS: Record<RailIcon, () => JSX.Element> = {
  // A window with a column at its left.
  workspaces: () => (
    <>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M6 3v10" />
    </>
  ),
  // A tray.
  inbox: () => (
    <path d="M2.5 9.5 4 4.4a1.5 1.5 0 0 1 1.4-1h5.2a1.5 1.5 0 0 1 1.4 1l1.5 5.1V12a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 12zM2.5 9.5h3l1 1.5h3l1-1.5h3" />
  ),
  // A line with three points and the rows beside them.
  timeline: () => (
    <>
      <path d="M4.5 2.5v11M8 4h5.5M8 8h5.5M8 12h3.5" />
      <circle cx="4.5" cy="4" r="1.25" fill="currentColor" />
      <circle cx="4.5" cy="8" r="1.25" fill="currentColor" />
      <circle cx="4.5" cy="12" r="1.25" fill="currentColor" />
    </>
  ),
  // A box with a check mark.
  tasks: () => (
    <>
      <rect x="2.5" y="2.5" width="11" height="11" rx="2" />
      <path d="m5.5 8.2 1.8 1.8 3.4-3.6" />
    </>
  ),
  // A folder.
  files: () => (
    <path d="M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z" />
  ),
  // Two branches of a tree.
  changes: () => (
    <>
      <circle cx="5" cy="3.75" r="1.5" />
      <circle cx="5" cy="12.25" r="1.5" />
      <circle cx="11" cy="5.25" r="1.5" />
      <path d="M5 5.25v5.5M11 6.75c0 2.5-6 1.5-6 4" />
    </>
  ),
};

export function Icon(props: { name: RailIcon }) {
  return (
    <svg
      viewBox="0 0 16 16"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {PATHS[props.name]()}
    </svg>
  );
}

/** A magnifier: the mark of a field that searches, at the start of the field. */
export function SearchIcon() {
  return (
    <svg
      class="search-icon"
      viewBox="0 0 16 16"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      aria-hidden="true"
    >
      <circle cx="7" cy="7" r="4.25" />
      <path d="m10.25 10.25 3.25 3.25" />
    </svg>
  );
}
