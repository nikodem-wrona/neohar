// The Popover of shadcn/ui (ui.shadcn.com), in React, on the tokens of the chrome: a glass panel in
// the top layer under the button that opens it. The button names the panel with `popoverTarget`,
// so a click on the button opens and closes the panel, and Escape or a click outside closes it. The
// panel opens under the button, with its right edge at the right edge of the button (`end`) or its
// left edge at the left edge of the button (`start`), at the place that the page measures when it
// opens. `onToggle` says when it opens and when it closes. The other attributes go to the element.

import { type ComponentProps, useCallback } from 'react';
import { useLatest } from '../lib/hooks';

export function Popover({
  id,
  align,
  onToggle,
  className,
  ...rest
}: {
  id: string;
  align?: 'start' | 'end' | undefined;
  onToggle?: ((open: boolean) => void) | undefined;
} & Omit<ComponentProps<'div'>, 'id' | 'onToggle'>) {
  const latest = useLatest({ id, onToggle });

  // The native events of the popover: the panel measures the button before it opens.
  const attach = useCallback(
    (panel: HTMLDivElement | null) => {
      if (!panel) return;
      const place = (): void => {
        const trigger = document.querySelector(
          `[popovertarget="${CSS.escape(latest.current.id)}"]`,
        );
        if (!trigger) return;
        const box = trigger.getBoundingClientRect();
        panel.style.setProperty('--ui-popover-top', `${box.bottom + 4}px`);
        panel.style.setProperty('--ui-popover-left', `${Math.max(0, box.left)}px`);
        panel.style.setProperty(
          '--ui-popover-right',
          `${Math.max(0, window.innerWidth - box.right)}px`,
        );
      };
      const beforeToggle = (event: Event): void => {
        if ((event as ToggleEvent).newState === 'open') place();
      };
      const toggle = (event: Event): void => {
        latest.current.onToggle?.((event as ToggleEvent).newState === 'open');
      };
      panel.addEventListener('beforetoggle', beforeToggle);
      panel.addEventListener('toggle', toggle);
      return () => {
        panel.removeEventListener('beforetoggle', beforeToggle);
        panel.removeEventListener('toggle', toggle);
      };
    },
    [latest],
  );

  return (
    <div
      id={id}
      className={className ? `ui-popover ${className}` : 'ui-popover'}
      data-align={align ?? 'end'}
      popover="auto"
      ref={attach}
      {...rest}
    />
  );
}
