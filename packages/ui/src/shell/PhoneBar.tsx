// The bottom bar of the desk on a phone in portrait: a button for each tab of the
// drawer, with the count of the Inbox, and More. A tap opens the drawer as a sheet at that tab, and
// a second tap closes it. The bar is over the safe area of the bottom.

import { For, Show } from 'solid-js';
import { useApp } from '../app/context';
import type { DrawerTab } from '../app/view';
import { TABS } from './Drawer';
import { Icon } from './icons';

export function PhoneBar() {
  const { stores, view } = useApp();
  const waiting = () => stores.attention.state.order.length;
  const showing = (tab: DrawerTab) => view.drawerOpen() && view.drawerTab() === tab;
  const toggle = (tab: DrawerTab): void => {
    view.setSheet(null);
    if (showing(tab)) view.setDrawerOpen(false);
    else view.showDrawer(tab);
  };
  const more = () => view.sheet()?.kind === 'more';
  return (
    <nav class="phone-dock" aria-label="Drawer">
      <For each={TABS}>
        {(tab) => (
          <button
            type="button"
            class="phone-dock-button"
            classList={{ hot: tab.id === 'inbox' && waiting() > 0 }}
            aria-pressed={showing(tab.id)}
            data-tab={tab.id}
            onClick={() => toggle(tab.id)}
          >
            <Icon name={tab.id} />
            <span class="phone-dock-label">{tab.label}</span>
            <Show when={tab.id === 'inbox' && waiting() > 0}>
              <span class="phone-dock-count" aria-label={`${waiting()} waiting`}>
                {waiting()}
              </span>
            </Show>
          </button>
        )}
      </For>
      <button
        type="button"
        class="phone-dock-button"
        aria-pressed={more()}
        data-tab="more"
        onClick={() => {
          view.setDrawerOpen(false);
          view.setSheet(more() ? null : { kind: 'more' });
        }}
      >
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
          <circle cx="3" cy="8" r="1.5" fill="currentColor" />
          <circle cx="8" cy="8" r="1.5" fill="currentColor" />
          <circle cx="13" cy="8" r="1.5" fill="currentColor" />
        </svg>
        <span class="phone-dock-label">More</span>
      </button>
    </nav>
  );
}
