// A long press of a finger on a part of the chrome: iOS gives no `contextmenu` for it
// **[verify]**, and a phone has no right click and no double click. A press of 500 ms that does not
// move opens the menu of the part. The click that follows the release does not reach the part.

import { LONG_PRESS_MS, SLOP } from '../input/touch';
import { realTimers } from '../lib/timing';
import { swallowClick } from './drag';

/**
 * Follows a press of a finger: after 500 ms without a move, `open` gets the point of the press. A
 * mouse and a pen do nothing here: they have a right click and a double click.
 */
export function onLongPress(press: PointerEvent, open: (x: number, y: number) => void): void {
  if (press.pointerType !== 'touch') return;
  const stop = (): void => {
    realTimers.clear(timer);
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', stop, true);
    window.removeEventListener('pointercancel', stop, true);
  };
  const move = (event: PointerEvent): void => {
    if (event.pointerId !== press.pointerId) return;
    if (Math.hypot(event.clientX - press.clientX, event.clientY - press.clientY) > SLOP) stop();
  };
  const timer = realTimers.set(() => {
    stop();
    swallowClick();
    open(press.clientX, press.clientY);
  }, LONG_PRESS_MS);
  window.addEventListener('pointermove', move, true);
  window.addEventListener('pointerup', stop, true);
  window.addEventListener('pointercancel', stop, true);
}
