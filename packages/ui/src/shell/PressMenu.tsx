// The menu of a long press on a phone: the actions of a tab, a workspace or a pane,
// where the desk has a right click, a double click or a hover. A tap on an item runs it, and a tap
// beside the menu or Escape closes it.

import { useApp } from '../app/context';
import { useValue } from '../lib/hooks';
import { cn } from '../lib/utils';

/** The width of the menu, for its place on the screen, in CSS pixels. */
const WIDTH = 220;

export function PressMenu() {
  const { view } = useApp();
  const menu = useValue(view.pressMenu);
  const close = (): void => {
    view.pressMenu.set(null);
  };
  return menu ? (
    <>
      <div className="press-scrim" aria-hidden="true" onClick={close} />
      <div
        className="press-menu"
        role="menu"
        aria-label={`Actions of ${menu.title}`}
        style={{
          left: `${Math.max(8, Math.min(menu.x, window.innerWidth - WIDTH - 8))}px`,
          top: `${Math.max(8, Math.min(menu.y, window.innerHeight - menu.items.length * 44 - 16))}px`,
          width: `${WIDTH}px`,
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') close();
        }}
      >
        {/* The labels of a menu are its keys: no two items of a menu have the same label. */}
        {menu.items.map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            className={cn('press-item', { danger: item.danger === true })}
            onClick={() => {
              close();
              item.run();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </>
  ) : null;
}
