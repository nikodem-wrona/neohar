// The focus after a layer of shadcn/ui that a store opened, not a trigger of Radix: the question of
// a close, the menu of a long press and the sheets of a phone. Radix gives the focus back to the
// trigger of a layer, and these layers have none. The layer keeps the element that had the focus
// when it opened, and gives the focus back to it when it closes.

import { useLayoutEffect, useRef } from 'react';

/**
 * The handler for `onCloseAutoFocus` of a layer that shows while `open` is true. With `always`,
 * the element gets the focus back in every case. Else it gets it back only when the focus went to
 * nothing, so that an action of the layer that gave the focus to a field (Rename) keeps it there.
 */
export function useFocusReturn(open: boolean, always = false): (event: Event) => void {
  const saved = useRef<HTMLElement | null>(null);
  // A layout effect runs before Radix moves the focus into the layer.
  useLayoutEffect(() => {
    if (!open) return;
    const element = document.activeElement;
    saved.current = element instanceof HTMLElement && element !== document.body ? element : null;
  }, [open]);
  return (event) => {
    event.preventDefault();
    const back = saved.current;
    saved.current = null;
    const now = document.activeElement;
    if (!always && now !== null && now !== document.body) return;
    if (back?.isConnected) back.focus({ preventScroll: true });
  };
}
