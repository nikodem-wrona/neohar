// The menu of a long press on a phone: the actions of a tab, a workspace or a pane,
// where the desk has a right click, a double click or a hover. A tap on an item runs it, and a tap
// beside the menu or Escape closes it.

import { For, Show } from 'solid-js';
import { useApp } from '../app/context';

/** The width of the menu, for its place on the screen, in CSS pixels. */
const WIDTH = 220;

export function PressMenu() {
  const { view } = useApp();
  const close = (): void => {
    view.setPressMenu(null);
  };
  return (
    <Show when={view.pressMenu()}>
      {(menu) => (
        <>
          <div class="press-scrim" aria-hidden="true" onClick={close} />
          <div
            class="press-menu"
            role="menu"
            aria-label={`Actions of ${menu().title}`}
            style={{
              left: `${Math.max(8, Math.min(menu().x, window.innerWidth - WIDTH - 8))}px`,
              top: `${Math.max(8, Math.min(menu().y, window.innerHeight - menu().items.length * 44 - 16))}px`,
              width: `${WIDTH}px`,
            }}
            onKeyDown={(event) => {
              if (event.key === 'Escape') close();
            }}
          >
            <For each={menu().items}>
              {(item) => (
                <button
                  type="button"
                  role="menuitem"
                  class="press-item"
                  classList={{ danger: item.danger === true }}
                  onClick={() => {
                    close();
                    item.run();
                  }}
                >
                  {item.label}
                </button>
              )}
            </For>
          </div>
        </>
      )}
    </Show>
  );
}
