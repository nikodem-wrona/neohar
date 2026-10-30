// The drawer: four tabs for Inbox, Timeline, Files and Changes. The inbox has the cards, the
// timeline has the turns of the agent in focus, the Files tab has the tree of the folder of the
// tab, and the Changes tab has the changes of the git worktree that holds that folder.
// Which tab shows is the state of this view only (`app/view.ts`). A folded drawer is a rail with a
// button for each tab: its icon, with the name as the tooltip. The Inbox button is amber and
// says how many agents wait. On the desk the drawer never
// covers a pane: open, it takes columns from the panes. On a phone in portrait it is a sheet over
// the panes, because the screen has no room beside them.

import { useApp } from '../app/context';
import type { DrawerTab } from '../app/view';
import { ChangesPanel } from '../changes/ChangesPanel';
import { Inbox } from '../drawer/Inbox';
import { FilesPanel } from '../files/FilesPanel';
import { useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { TimelinePanel } from '../timeline/TimelinePanel';
import { Icon } from './icons';
import { Tip } from './Tip';

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
function useWaiting(): number {
  const { stores } = useApp();
  return useSelect(stores.attention, (state) => state.order.length);
}

export function Drawer() {
  const { view } = useApp();
  const active = useValue(view.drawerTab);
  const setActive = (tab: DrawerTab): void => view.drawerTab.set(tab);
  const waiting = useWaiting();
  return (
    <aside className="drawer" aria-label="Drawer">
      <div className="drawer-tabs">
        <div className="drawer-tablist" role="tablist">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              className={cn('drawer-tab', { selected: active === tab.id })}
              aria-selected={active === tab.id}
              title={tab.label}
              onClick={() => setActive(tab.id)}
            >
              {tab.label}
              {tab.id === 'inbox' && waiting > 0 ? (
                <span className="drawer-count">{waiting}</span>
              ) : null}
            </button>
          ))}
        </div>
        <Tip text="Fold the drawer to a rail">
          <button
            type="button"
            className="drawer-fold"
            aria-label="Fold"
            onClick={() => view.drawerOpen.set(false)}
          >
            <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
              <path d="M6 3l5 5-5 5" fill="none" stroke="currentColor" strokeWidth="1.5" />
            </svg>
          </button>
        </Tip>
      </div>
      <div className={cn('drawer-body', { fill: active === 'files' })} role="tabpanel">
        {active === 'inbox' ? <Inbox /> : null}
        {active === 'timeline' ? <TimelinePanel /> : null}
        {active === 'files' ? <FilesPanel /> : null}
        {active === 'changes' ? <ChangesPanel /> : null}
        {TABS.filter((tab) => !OWN_BODY.includes(tab.id)).map((tab) => (
          <p key={tab.id} className="drawer-empty" hidden={active !== tab.id}>
            {tab.empty}
          </p>
        ))}
      </div>
    </aside>
  );
}

/** The folded drawer: one button for each tab, and the number of agents that wait. */
export function DrawerRail() {
  const { view } = useApp();
  const waiting = useWaiting();
  return (
    <nav className="drawer-rail" aria-label="Folded drawer">
      {TABS.map((tab) => (
        <Tip key={tab.id} text={tab.label} side="left">
          <button
            type="button"
            className={cn('rail-button', { hot: tab.id === 'inbox' && waiting > 0 })}
            aria-label={
              tab.id === 'inbox' && waiting > 0
                ? `Show ${tab.label}, ${waiting} waiting`
                : `Show ${tab.label}`
            }
            onClick={() => view.showDrawer(tab.id)}
          >
            <Icon name={tab.id} />
            {tab.id === 'inbox' && waiting > 0 ? (
              <span className="rail-count">{waiting}</span>
            ) : null}
          </button>
        </Tip>
      ))}
    </nav>
  );
}
