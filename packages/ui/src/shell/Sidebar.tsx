// The left column: the workspaces with their tabs, and no handles. The agents are on the board.
// A click on a tab shows it in this window, and a pane that is dropped on a tab moves into
// it. The cross of a workspace or of a tab closes it. A double click on the name of a
// workspace or of a tab, or F2 on a tab, opens a field for a new name. The column folds to
// a rail, as the drawer does. On a phone, the list of the machines comes after the workspaces.
//
// The column stays a custom part made of the Buttons and Tooltips of shadcn/ui. The Sidebar of
// shadcn/ui is fixed to the edge of the window with a width in rem and its own open state and
// sheet, and this column lies in the grid of the shell, under the command line and the board,
// with a width in characters that the rule drags (`view.columns`) and a rail of 36 px.

import { type ReactNode, useState } from 'react';
import { useApp } from '../app/context';
import { Button } from '../components/ui/button';
import { dropTarget } from '../app/drag';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { requestClose, type CloseTarget } from './close';
import { CHROME_BUTTON, RAIL_BUTTON } from './chrome';
import { Icon } from './icons';
import { Tip } from './Tip';
import { onLongPress } from '../app/press';
import { RenameField } from './RenameField';
import { sameTarget, type RenameTarget } from './rename';

export function Sidebar(props: { extra?: () => ReactNode } = {}) {
  const { stores, transport, run, report, windowId, view, form } = useApp();
  const workspaces = useRows(stores.tree);
  const shownTab = useSelect(stores.windows, (state) => state.byKey[windowId]?.tab);
  const renaming = useValue(view.renaming);
  const select = (tab: string): void => {
    run('tab.select', { tab, window: windowId });
    // On a phone in portrait the left column is a sheet: the tab that the person picks shows.
    if (form === 'phone' && window.matchMedia('(orientation: portrait)').matches) {
      view.leftOpen.set(false);
    }
  };
  const create = () => run('workspace.create', {});
  const close = (target: CloseTarget): void => {
    requestClose(transport, target)
      .then((closing) => view.closing.set(closing))
      .catch(report);
  };
  const [over, setOver] = useState<string | null>(null);
  const edits = (kind: RenameTarget['kind'], id: string): RenameTarget | null => {
    const target: RenameTarget = { kind, id, place: 'left' };
    return sameTarget(renaming, target) ? target : null;
  };
  const rename = (kind: RenameTarget['kind'], id: string): void => {
    view.renaming.set({ kind, id, place: 'left' });
  };
  // A pane that is dropped on a tab of the left column moves into that tab.
  const drop = (pane: string, tab: string): void => {
    setOver(null);
    run('pane.move', { pane, tab });
  };
  return (
    <nav className="sidebar" aria-label="Workspaces">
      <div className="band">
        <span className="band-title">Workspaces</span>
        <span className="band-count">{workspaces.length}</span>
        <Tip text="Fold the workspaces to a rail">
          <Button
            variant="ghost"
            className={cn(
              CHROME_BUTTON,
              'column-fold -mr-1.5 ml-auto h-6.5 px-1.5 has-[>svg]:px-1.5',
            )}
            aria-label="Fold the left column"
            onClick={() => view.leftOpen.set(false)}
          >
            <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
              <path d="M10 3l-5 5 5 5" fill="none" stroke="currentColor" strokeWidth="1.5" />
            </svg>
          </Button>
        </Tip>
      </div>
      <ul className="workspaces">
        {workspaces.map((workspace) => {
          const editsWorkspace = edits('workspace', workspace.id);
          return (
            <li key={workspace.id} className="workspace" data-workspace={workspace.id}>
              <div className="workspace-name group/row">
                {editsWorkspace ? (
                  <RenameField
                    target={editsWorkspace}
                    name={workspace.name}
                    label={`Name of workspace ${workspace.handle}`}
                    className="workspace-title"
                  />
                ) : (
                  <span
                    className="workspace-title"
                    onDoubleClick={() => rename('workspace', workspace.id)}
                    onPointerDown={(event) =>
                      // A phone has no double click: a long press opens the menu.
                      onLongPress(event.nativeEvent, (x, y) =>
                        view.pressMenu.set({
                          x,
                          y,
                          title: workspace.name,
                          items: [
                            { label: 'Rename', run: () => rename('workspace', workspace.id) },
                            {
                              label: 'Close',
                              danger: true,
                              run: () =>
                                close({
                                  kind: 'workspace',
                                  id: workspace.id,
                                  name: workspace.name,
                                }),
                            },
                          ],
                        }),
                      )
                    }
                  >
                    {workspace.name}
                  </span>
                )}
                <Tip text="Close workspace">
                  <Button
                    variant="ghost"
                    className={cn(
                      CHROME_BUTTON,
                      'row-close size-4.5 opacity-50 group-hover/row:opacity-100 hover:text-destructive focus-visible:opacity-100 [@media(hover:none)]:opacity-100',
                    )}
                    data-action="workspace-close"
                    aria-label={`Close workspace ${workspace.name}`}
                    onClick={() =>
                      close({ kind: 'workspace', id: workspace.id, name: workspace.name })
                    }
                  >
                    <CloseIcon />
                  </Button>
                </Tip>
              </div>
              <ul className="workspace-tabs">
                {workspace.tabs.map((tab) => {
                  const editsTab = edits('tab', tab.id);
                  return (
                    <li key={tab.id} className="side-tab-row group/row">
                      {editsTab ? (
                        <RenameField
                          target={editsTab}
                          name={tab.name}
                          label={`Name of tab ${tab.handle}`}
                        />
                      ) : (
                        <Button
                          variant="ghost"
                          className={cn(
                            CHROME_BUTTON,
                            'side-tab block h-[calc(var(--row)-4px)] min-w-0 truncate rounded-(--r-tab) px-(--s2) text-left hover:bg-(--bg-hover) hover:text-muted-foreground dark:hover:bg-(--bg-hover)',
                            // The tab that the window shows, as the selected tab of the tab bar: a
                            // quiet "you are here", with no change of weight.
                            tab.id === shownTab &&
                              'selected bg-(--bg-hover) text-foreground shadow-[inset_0_0_0_1px_var(--hairline)] hover:text-foreground',
                            over === tab.id && 'drop-target shadow-[inset_0_0_0_1px_var(--text)]',
                          )}
                          data-side-tab={tab.id}
                          aria-keyshortcuts="F2"
                          onClick={() => select(tab.id)}
                          onDoubleClick={() => rename('tab', tab.id)}
                          onPointerDown={(event) =>
                            onLongPress(event.nativeEvent, (x, y) =>
                              view.pressMenu.set({
                                x,
                                y,
                                title: tab.name,
                                items: [
                                  { label: 'Rename', run: () => rename('tab', tab.id) },
                                  {
                                    label: 'Close',
                                    danger: true,
                                    run: () => close({ kind: 'tab', id: tab.id, name: tab.name }),
                                  },
                                ],
                              }),
                            )
                          }
                          onKeyDown={(event) => {
                            if (event.key !== 'F2') return;
                            event.preventDefault();
                            rename('tab', tab.id);
                          }}
                          ref={(element) => {
                            if (!element) return;
                            dropTarget(element, {
                              accepts: (item) => item.kind === 'pane',
                              enter: () => setOver(tab.id),
                              leave: () =>
                                setOver((current) => (current === tab.id ? null : current)),
                              drop: (item) => {
                                if (item.kind === 'pane') drop(item.id, tab.id);
                              },
                            });
                          }}
                        >
                          {tab.name}
                        </Button>
                      )}
                      <Tip text="Close tab">
                        <Button
                          variant="ghost"
                          className={cn(
                            CHROME_BUTTON,
                            'row-close size-4.5 opacity-50 group-hover/row:opacity-100 hover:text-destructive focus-visible:opacity-100 [@media(hover:none)]:opacity-100',
                          )}
                          data-action="tab-close"
                          aria-label={`Close tab ${tab.name}`}
                          onClick={() => close({ kind: 'tab', id: tab.id, name: tab.name })}
                        >
                          <CloseIcon />
                        </Button>
                      </Tip>
                    </li>
                  );
                })}
              </ul>
            </li>
          );
        })}
      </ul>
      <Button
        variant="ghost"
        className={cn(
          CHROME_BUTTON,
          "new-workspace mt-(--s2) block h-(--row) truncate px-(--s2) text-left before:content-['+_']",
        )}
        onClick={create}
      >
        New workspace
      </Button>
      {props.extra?.()}
    </nav>
  );
}

/** The folded left column: a rail with the Workspaces button. */
export function LeftRail() {
  const { view } = useApp();
  return (
    <nav className="left-rail" aria-label="Folded left column">
      <Tip text="Workspaces" side="right">
        <Button
          variant="ghost"
          className={cn(CHROME_BUTTON, RAIL_BUTTON)}
          aria-label="Show Workspaces"
          onClick={() => view.leftOpen.set(true)}
        >
          <Icon name="workspaces" />
        </Button>
      </Tip>
    </nav>
  );
}

function CloseIcon() {
  return (
    <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}
