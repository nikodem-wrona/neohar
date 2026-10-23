// The drawer: four tabs for Inbox, Timeline, Files and Changes. The inbox has the cards, the
// timeline has the turns of the agent in focus, the Files tab has the tree of the folder of the
// tab, and the Changes tab has the changes of the git worktree that holds that folder.
// Which tab shows is the state of this view only (`app/view.ts`). A folded drawer is a rail with a
// button for each tab: its icon, with the name as the tooltip. The Inbox button is amber and
// says how many agents wait. On the desk the drawer never
// covers a pane: open, it takes columns from the panes. On a phone in portrait it is a sheet over
// the panes, because the screen has no room beside them.

import { For, Show } from 'solid-js';
import { useApp } from '../app/context';
import type { DrawerTab } from '../app/view';
import { ChangesPanel } from '../changes/ChangesPanel';
import { Inbox } from '../drawer/Inbox';
import { FilesPanel } from '../files/FilesPanel';
import { TimelinePanel } from '../timeline/TimelinePanel';
import { Icon } from './icons';

/** The tabs of the drawer, in their order. The bottom bar of a phone has them too. */
export const TABS: readonly { id: DrawerTab; label: string; empty: string }[] = [
  { id: 'inbox', label: 'Inbox', empty: 'Nothing waits for an answer.' },
  { id: 'timeline', label: 'Timeline', empty: 'No agent is in focus.' },
  { id: 'files', label: 'Files', empty: 'This tab has no folder.' },
  { id: 'changes', label: 'Changes', empty: 'This tab has no folder.' },
];

/** The tabs whose body is a component of their own. */
const OWN_BODY: readonly DrawerTab[] = ['inbox', 'timeline', 'files', 'changes'];

/** How many agents wait for a person, as the `attention` projection counts them. */
function useWaiting(): () => number {
  const { stores } = useApp();
  return () => stores.attention.state.order.length;
}

export function Drawer() {
  const { view } = useApp();
  const active = view.drawerTab;
  const setActive = view.setDrawerTab;
  const waiting = useWaiting();
  return (
    <aside class="drawer" aria-label="Drawer">
      <div class="drawer-tabs">
        <div class="drawer-tablist" role="tablist">
          <For each={TABS}>
            {(tab) => (
              <button
                type="button"
                role="tab"
                class="drawer-tab"
                classList={{ selected: active() === tab.id }}
                aria-selected={active() === tab.id}
                title={tab.label}
                onClick={() => setActive(tab.id)}
              >
                {tab.label}
                <Show when={tab.id === 'inbox' && waiting() > 0}>
                  <span class="drawer-count">{waiting()}</span>
                </Show>
              </button>
            )}
          </For>
        </div>
        <button
          type="button"
          class="drawer-fold"
          aria-label="Fold"
          title="Fold the drawer to a rail"
          onClick={() => view.setDrawerOpen(false)}
        >
          <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
            <path d="M6 3l5 5-5 5" fill="none" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>
      <div class="drawer-body" classList={{ fill: active() === 'files' }} role="tabpanel">
        <Show when={active() === 'inbox'}>
          <Inbox />
        </Show>
        <Show when={active() === 'timeline'}>
          <TimelinePanel />
        </Show>
        <Show when={active() === 'files'}>
          <FilesPanel />
        </Show>
        <Show when={active() === 'changes'}>
          <ChangesPanel />
        </Show>
        <For each={TABS.filter((tab) => !OWN_BODY.includes(tab.id))}>
          {(tab) => (
            <p class="drawer-empty" hidden={active() !== tab.id}>
              {tab.empty}
            </p>
          )}
        </For>
      </div>
    </aside>
  );
}

/** The folded drawer: one button for each tab, and the number of agents that wait. */
export function DrawerRail() {
  const { view } = useApp();
  const waiting = useWaiting();
  return (
    <nav class="drawer-rail" aria-label="Folded drawer">
      <For each={TABS}>
        {(tab) => (
          <button
            type="button"
            class="rail-button"
            classList={{ hot: tab.id === 'inbox' && waiting() > 0 }}
            aria-label={
              tab.id === 'inbox' && waiting() > 0
                ? `Show ${tab.label}, ${waiting()} waiting`
                : `Show ${tab.label}`
            }
            title={tab.label}
            onClick={() => view.showDrawer(tab.id)}
          >
            <Icon name={tab.id} />
            <Show when={tab.id === 'inbox' && waiting() > 0}>
              <span class="rail-count">{waiting()}</span>
            </Show>
          </button>
        )}
      </For>
    </nav>
  );
}
