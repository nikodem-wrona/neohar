// The menu of a long press on a phone: the actions of a tab, a workspace or a row of the Files tab,
// where the desk has a right click, a double click or a hover. A tap on an item runs it, and a tap
// beside the menu or Escape closes it.
//
// It is the Dropdown Menu of shadcn/ui at the point of the press. The page finds the long press
// (`app/press.ts`) and puts the menu in the view, so the trigger of Radix is an empty element at
// that point: the Context Menu of Radix cannot open from the view, and its own long press would
// start beside the drag of a tab. The menu is not modal: a modal menu of Radix hides the parts of
// the shell beside the live regions from a screen reader, and they keep their buttons. A scrim
// takes the tap beside the menu instead, so that the tap does not reach what is under it.

import { useRef, useState } from 'react';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import type { PressMenu as Menu } from '../app/view';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../components/ui/dropdown-menu';
import { useValue } from '../lib/hooks';
import { useFocusReturn } from './focus';

/** The width of the menu, in CSS pixels. */
const WIDTH = 220;

export function PressMenu() {
  const { view } = useApp();
  const menu = useValue(view.pressMenu);
  // The last menu, so that the items stay while the menu closes.
  const [shown, setShown] = useState<Menu | null>(menu);
  if (menu !== null && menu !== shown) setShown(menu);
  const open = menu !== null;
  useLayer(open);
  const focusBack = useFocusReturn(open);
  const scrim = useRef<HTMLDivElement>(null);
  const close = (): void => {
    view.pressMenu.set(null);
  };
  return (
    <DropdownMenu
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      modal={false}
    >
      {open ? (
        <div ref={scrim} className="fixed inset-0 z-50" aria-hidden="true" onClick={close} />
      ) : null}
      <DropdownMenuTrigger asChild>
        <span
          className="pointer-events-none fixed size-0"
          style={{ left: `${shown?.x ?? 0}px`, top: `${shown?.y ?? 0}px` }}
          aria-hidden="true"
        />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        collisionPadding={8}
        style={{ width: `${WIDTH}px` }}
        aria-label={`Actions of ${shown?.title ?? ''}`}
        // The name is the title of the part, not the empty trigger.
        aria-labelledby={undefined}
        onCloseAutoFocus={focusBack}
        // The scrim closes the menu with the click, after the press.
        onPointerDownOutside={(event) => {
          if (event.target === scrim.current) event.preventDefault();
        }}
      >
        {/* The labels of a menu are its keys: no two items of a menu have the same label. */}
        {shown?.items.map((item) => (
          <DropdownMenuItem
            key={item.label}
            variant={item.danger === true ? 'destructive' : 'default'}
            className="min-h-(--control) px-3"
            onSelect={() => item.run()}
          >
            {item.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
