// The Popover of shadcn/ui (ui.shadcn.com), in Solid, on the tokens of the chrome: a glass panel in
// the top layer under the button that opens it. The button names the panel with `popovertarget`,
// so a click on the button opens and closes the panel, and Escape or a click outside closes it. The
// panel opens under the button, with its right edge at the right edge of the button (`end`) or its
// left edge at the left edge of the button (`start`), at the place that the page measures when it
// opens. `onToggle` says when it opens and when it closes. The other attributes go to the element.

import { type JSX, splitProps } from 'solid-js';

export function Popover(
  props: {
    id: string;
    align?: 'start' | 'end' | undefined;
    onToggle?: ((open: boolean) => void) | undefined;
  } & Omit<JSX.HTMLAttributes<HTMLDivElement>, 'id' | 'onToggle'>,
) {
  const [own, rest] = splitProps(props, ['id', 'align', 'onToggle', 'class']);
  const align = () => own.align ?? 'end';

  const place = (panel: HTMLElement): void => {
    const trigger = document.querySelector(`[popovertarget="${CSS.escape(own.id)}"]`);
    if (!trigger) return;
    const box = trigger.getBoundingClientRect();
    panel.style.setProperty('--ui-popover-top', `${box.bottom + 4}px`);
    panel.style.setProperty('--ui-popover-left', `${Math.max(0, box.left)}px`);
    panel.style.setProperty(
      '--ui-popover-right',
      `${Math.max(0, window.innerWidth - box.right)}px`,
    );
  };

  return (
    <div
      id={own.id}
      class={own.class ? `ui-popover ${own.class}` : 'ui-popover'}
      data-align={align()}
      popover="auto"
      ref={(panel) => {
        panel.addEventListener('beforetoggle', (event) => {
          if ((event as ToggleEvent).newState === 'open') place(panel);
        });
        panel.addEventListener('toggle', (event) => {
          own.onToggle?.((event as ToggleEvent).newState === 'open');
        });
      }}
      {...rest}
    />
  );
}
