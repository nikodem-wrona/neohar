// A drag of a tab or a pane onto a tab, of a strip of the board onto another strip, and of a row
// of the Files tab onto a pane. The page follows the pointer with pointer events and finds
// the target under it. The drag events of HTML do not come to the page in the app: the drag
// handler of the host takes every drag that comes into the window, for the files that are dropped
// on a pane. Only the id of a tab, a pane or a run is kept, and the core does the move
// (`tab.move`, `pane.move`, `agent.move`). A row of the Files tab carries its path, and the core
// pastes it (`pane.insert_paths`).
//
// A finger drags only after a long press: a finger that moves before it scrolls the
// list, and the press is no drag.

import { createSignal } from 'solid-js';
import { LONG_PRESS_MS } from '../input/touch';
import { realTimers } from '../lib/timing';

export type Dragged =
  | { kind: 'tab'; id: string }
  | { kind: 'pane'; id: string }
  | { kind: 'agent'; id: string }
  | { kind: 'path'; path: string };

/** An element that takes a drop. */
export interface DropTarget {
  accepts: (item: Dragged) => boolean;
  /** The pointer came over the element with an item that it accepts. */
  enter: () => void;
  leave: () => void;
  drop: (item: Dragged) => void;
}

/** How far the pointer moves with the button down before a press is a drag, in CSS pixels. */
export const DRAG_THRESHOLD = 4;

const [dragged, setDragged] = createSignal<Dragged | null>(null);
const targets = new WeakMap<Element, DropTarget>();

/** What the person drags now, or null. */
export { dragged };

/** Makes `element` a target of drops. */
export function dropTarget(element: Element, target: DropTarget): void {
  targets.set(element, target);
}

/** Whether a pointer that moved by `dx`, `dy` from the press is a drag. */
export function isDrag(dx: number, dy: number): boolean {
  return dx * dx + dy * dy >= DRAG_THRESHOLD * DRAG_THRESHOLD;
}

/** The nearest target at the point, if it accepts the item. */
function targetAt(x: number, y: number, item: Dragged): Element | null {
  for (let element = document.elementFromPoint(x, y); element; element = element.parentElement) {
    const target = targets.get(element);
    if (target) return target.accepts(item) ? element : null;
  }
  return null;
}

/**
 * Follows a press of the main button that can become a drag of `item`. A press that does not move
 * stays a click. A release over a target drops the item there, and a release anywhere else does
 * nothing. The click that comes after the release of a drag does not reach the page.
 */
export function pressToDrag(press: PointerEvent, item: Dragged): void {
  if (press.button !== 0 || dragged()) return;
  let started = false;
  // A finger: the drag waits for a long press.
  const armed = press.pointerType === 'touch' ? press.timeStamp + LONG_PRESS_MS : 0;
  let over: Element | null = null;
  const hover = (next: Element | null): void => {
    if (next === over) return;
    if (over) targets.get(over)?.leave();
    over = next;
    if (over) targets.get(over)?.enter();
  };
  const move = (event: PointerEvent): void => {
    if (event.pointerId !== press.pointerId) return;
    if (!started) {
      if (!isDrag(event.clientX - press.clientX, event.clientY - press.clientY)) return;
      if (event.timeStamp < armed) {
        // The finger moved before the long press: a scroll, not a drag.
        finish(event);
        return;
      }
      started = true;
      setDragged(item);
    }
    hover(targetAt(event.clientX, event.clientY, item));
  };
  const finish = (event: PointerEvent): void => {
    if (event.pointerId !== press.pointerId) return;
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', finish, true);
    window.removeEventListener('pointercancel', finish, true);
    const target = over;
    hover(null);
    setDragged(null);
    if (!started) return;
    swallowClick();
    if (event.type === 'pointerup' && target) targets.get(target)?.drop(item);
  };
  window.addEventListener('pointermove', move, true);
  window.addEventListener('pointerup', finish, true);
  window.addEventListener('pointercancel', finish, true);
}

/** Stops the click that the browser sends after the release, so that a drag does not select. */
export function swallowClick(): void {
  const stop = (event: Event): void => {
    event.stopPropagation();
    event.preventDefault();
  };
  window.addEventListener('click', stop, { capture: true, once: true });
  // The click comes in the same task as the release. A release with no click leaves no listener.
  realTimers.set(() => window.removeEventListener('click', stop, true), 0);
}
