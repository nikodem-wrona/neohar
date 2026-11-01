// The drawer: four tabs for Inbox, Timeline, Files and Changes. The inbox has the cards, the
// timeline has the turns of the agent in focus, the Files tab has the tree of the folder of the
// tab, and the Changes tab has the changes of the git worktree that holds that folder.
// Which tab shows is the state of this view only (`app/view.ts`). A folded drawer is a rail with a
// button for each tab: its icon, with the name as the tooltip. The Inbox button is amber and
// says how many agents wait. On the desk the drawer never
// covers a pane: open, it takes columns from the panes. On a phone in portrait it is a sheet over
// the panes, because the screen has no room beside them.

import type { ReactNode } from 'react';
import { useApp } from '../app/context';
import type { DrawerTab } from '../app/view';
import { ChangesPanel } from '../changes/ChangesPanel';
import { ScrollArea } from '../components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../components/ui/tabs';
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

/** How many agents wait for a person, as the `attention` projection counts them. */
function useWaiting(): number {
  const { stores } = useApp();
  return useSelect(stores.attention, (state) => state.order.length);
}

/** The padding of the body of a tab. */
const BODY = 'px-(--s4) pt-(--s2) pb-(--s4)';

/**
 * The panel of a tab: a column that fills the drawer. Radix keeps the panels of the other tabs
 * in the page with `hidden`, which a class for `display` would undo.
 */
const PANEL = 'flex min-h-0 flex-col data-[state=inactive]:hidden';

/**
 * The body of a tab whose list scrolls: the Scroll Area of shadcn/ui. Its content keeps the width
 * of the drawer, so a long path cuts with an ellipsis and does not scroll to the side.
 */
function Scrolled(props: { children: ReactNode }) {
  return (
    <ScrollArea className="min-h-0 flex-1 [&_[data-slot=scroll-area-viewport]>div]:block!">
      <div className={BODY}>{props.children}</div>
    </ScrollArea>
  );
}

export function Drawer() {
  const { view } = useApp();
  const active = useValue(view.drawerTab);
  const waiting = useWaiting();
  return (
    <aside className="drawer" aria-label="Drawer">
      {/* The Tabs of shadcn/ui. Which tab shows is the value of the view. */}
      <Tabs
        value={active}
        onValueChange={(tab) => view.drawerTab.set(tab as DrawerTab)}
        className="min-h-0 flex-1 gap-0"
      >
        <div className="flex h-11 flex-none items-center px-(--s2)">
          <TabsList className="h-auto min-w-0 flex-1 justify-start bg-transparent p-0 group-data-[orientation=horizontal]/tabs:h-auto">
            {TABS.map((tab) => (
              // Four tabs do not fit a narrow drawer: a tab that is not selected gets narrower and
              // cuts its label, and the selected tab keeps its label. The selected tab is a card
              // on the panel, in bold, with no colour.
              <TabsTrigger
                key={tab.id}
                value={tab.id}
                title={tab.label}
                className="h-(--row) min-h-(--control) min-w-[3ch] flex-[0_1_auto] justify-start rounded-(--r-tab) border-0 px-(--s2) font-normal text-muted-foreground hover:bg-secondary hover:text-foreground data-[state=active]:shrink-0 data-[state=active]:bg-card data-[state=active]:font-semibold data-[state=active]:text-foreground group-data-[variant=default]/tabs-list:data-[state=active]:shadow-[inset_0_0_0_1px_var(--hairline)] dark:data-[state=active]:border-transparent dark:data-[state=active]:bg-card"
              >
                <span className="truncate">{tab.label}</span>
                {tab.id === 'inbox' && waiting > 0 ? (
                  <span className="text-caution">{waiting}</span>
                ) : null}
              </TabsTrigger>
            ))}
          </TabsList>
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
        <TabsContent value="inbox" className={PANEL}>
          <Scrolled>
            <Inbox />
          </Scrolled>
        </TabsContent>
        <TabsContent value="timeline" className={PANEL}>
          <Scrolled>
            <TimelinePanel />
          </Scrolled>
        </TabsContent>
        {/* The Files tab gives its height to the panel, which scrolls the tree and the file. */}
        <TabsContent value="files" className={cn(PANEL, 'overflow-hidden', BODY)}>
          <FilesPanel />
        </TabsContent>
        <TabsContent value="changes" className={PANEL}>
          <Scrolled>
            <ChangesPanel />
          </Scrolled>
        </TabsContent>
      </Tabs>
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
