// The bottom bar of the desk on a phone in portrait: a button for each tab of the
// drawer, with the count of the Inbox, and More. A tap opens the drawer as a sheet at that tab, and
// a second tap closes it. The bar is over the safe area of the bottom.

import { useApp } from '../app/context';
import type { DrawerTab } from '../app/view';
import { useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { TABS } from './Drawer';
import { Icon } from './icons';

export function PhoneBar() {
  const { stores, view } = useApp();
  const waiting = useSelect(stores.attention, (state) => state.order.length);
  const drawerOpen = useValue(view.drawerOpen);
  const drawerTab = useValue(view.drawerTab);
  const sheet = useValue(view.sheet);
  const showing = (tab: DrawerTab) => drawerOpen && drawerTab === tab;
  const toggle = (tab: DrawerTab): void => {
    view.sheet.set(null);
    if (view.drawerOpen.get() && view.drawerTab.get() === tab) view.drawerOpen.set(false);
    else view.showDrawer(tab);
  };
  const more = sheet?.kind === 'more';
  return (
    <nav className="phone-dock" aria-label="Drawer">
      {TABS.map((tab) => (
        <button
          key={tab.id}
          type="button"
          className={cn('phone-dock-button', { hot: tab.id === 'inbox' && waiting > 0 })}
          aria-pressed={showing(tab.id)}
          data-tab={tab.id}
          onClick={() => toggle(tab.id)}
        >
          <Icon name={tab.id} />
          <span className="phone-dock-label">{tab.label}</span>
          {tab.id === 'inbox' && waiting > 0 ? (
            <span className="phone-dock-count" aria-label={`${waiting} waiting`}>
              {waiting}
            </span>
          ) : null}
        </button>
      ))}
      <button
        type="button"
        className="phone-dock-button"
        aria-pressed={more}
        data-tab="more"
        onClick={() => {
          view.drawerOpen.set(false);
          view.sheet.set(view.sheet.get()?.kind === 'more' ? null : { kind: 'more' });
        }}
      >
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
          <circle cx="3" cy="8" r="1.5" fill="currentColor" />
          <circle cx="8" cy="8" r="1.5" fill="currentColor" />
          <circle cx="13" cy="8" r="1.5" fill="currentColor" />
        </svg>
        <span className="phone-dock-label">More</span>
      </button>
    </nav>
  );
}
