// Where a reserved chord goes: the next tab, and the pane next to a pane on the screen. The core
// owns the order of the tabs and the layout. These functions only read the boxes that the page
// shows, to find the neighbour in a direction.

export type Direction = 'left' | 'right' | 'up' | 'down';

/** The tab after or before `current`, with a wrap at both ends. */
export function neighborTab(
  tabs: readonly { id: string }[],
  current: string | null | undefined,
  step: 1 | -1,
): string | undefined {
  if (tabs.length === 0) return undefined;
  const at = tabs.findIndex((tab) => tab.id === current);
  const next =
    at < 0 ? (step === 1 ? 0 : tabs.length - 1) : (at + step + tabs.length) % tabs.length;
  return tabs[next]?.id;
}

/** A pane as a box on the screen. */
export interface Box {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * The pane next to `from` in a direction. A pane counts when its centre lies past the edge of
 * `from` in that direction. The nearest wins, and a pane that lines up with `from` beats a pane
 * that sits at the side.
 */
export function nearestPane(
  boxes: readonly Box[],
  from: string,
  direction: Direction,
): string | undefined {
  const origin = boxes.find((box) => box.id === from);
  if (!origin) return undefined;
  const horizontal = direction === 'left' || direction === 'right';
  const forward = direction === 'right' || direction === 'down';
  const edge = horizontal
    ? forward
      ? origin.x + origin.w
      : origin.x
    : forward
      ? origin.y + origin.h
      : origin.y;
  const centreOf = (box: Box) => ({ x: box.x + box.w / 2, y: box.y + box.h / 2 });
  const here = centreOf(origin);
  let best: { id: string; score: number } | undefined;
  for (const box of boxes) {
    if (box.id === from) continue;
    const there = centreOf(box);
    const along = horizontal ? there.x : there.y;
    const past = forward ? along > edge : along < edge;
    if (!past) continue;
    const gap = Math.abs(along - edge);
    const aside = Math.abs(horizontal ? there.y - here.y : there.x - here.x);
    const score = gap + 2 * aside;
    if (!best || score < best.score) best = { id: box.id, score };
  }
  return best?.id;
}
