// The left column: the workspaces with their tabs, and no handles. The agents are on the board.
// A click on a tab shows it in this window, and a pane that is dropped on a tab moves into
// it. The cross of a workspace or of a tab closes it. A double click on the name of a
// workspace or of a tab, or F2 on a tab, opens a field for a new name. The column folds to
// a rail, as the drawer does. On a phone, the list of the machines comes after the workspaces.

import { createSignal, For, type JSX, Show } from 'solid-js';
import { useApp } from '../app/context';
import { dropTarget } from '../app/drag';
import { requestClose, type CloseTarget } from './close';
import { Icon } from './icons';
import { onLongPress } from '../app/press';
import { RenameField } from './RenameField';
import { sameTarget, type RenameTarget } from './rename';

export function Sidebar(props: { extra?: () => JSX.Element } = {}) {
  const { stores, transport, run, report, windowId, view, form } = useApp();
  const shown = () => stores.windows.state.byKey[windowId];
  const select = (tab: string): void => {
    run('tab.select', { tab, window: windowId });
    // On a phone in portrait the left column is a sheet: the tab that the person picks shows.
    if (form === 'phone' && window.matchMedia('(orientation: portrait)').matches) {
      view.setLeftOpen(false);
    }
  };
  const create = () => run('workspace.create', {});
  const close = (target: CloseTarget): void => {
    requestClose(transport, target).then(view.setClosing).catch(report);
  };
  const [over, setOver] = createSignal<string | null>(null);
  const edits = (kind: RenameTarget['kind'], id: string): RenameTarget | null => {
    const target: RenameTarget = { kind, id, place: 'left' };
    return sameTarget(view.renaming(), target) ? target : null;
  };
  const rename = (kind: RenameTarget['kind'], id: string): void => {
    view.setRenaming({ kind, id, place: 'left' });
  };
  // A pane that is dropped on a tab of the left column moves into that tab.
  const drop = (pane: string, tab: string): void => {
    setOver(null);
    run('pane.move', { pane, tab });
  };
  return (
    <nav class="sidebar" aria-label="Workspaces">
      <div class="band">
        <span class="band-title">Workspaces</span>
        <span class="band-count">{stores.tree.rows().length}</span>
        <button
          type="button"
          class="column-fold"
          aria-label="Fold the left column"
          title="Fold the workspaces to a rail"
          onClick={() => view.setLeftOpen(false)}
        >
          <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
            <path d="M10 3l-5 5 5 5" fill="none" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>
      <ul class="workspaces">
        <For each={stores.tree.rows()}>
          {(workspace) => (
            <li class="workspace" data-workspace={workspace.id}>
              <div class="workspace-name">
                <Show
                  when={edits('workspace', workspace.id)}
                  fallback={
                    <span
                      class="workspace-title"
                      onDblClick={() => rename('workspace', workspace.id)}
                      onPointerDown={(event) =>
                        // A phone has no double click: a long press opens the menu.
                        onLongPress(event, (x, y) =>
                          view.setPressMenu({
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
                  }
                >
                  {(target) => (
                    <RenameField
                      target={target()}
                      name={workspace.name}
                      label={`Name of workspace ${workspace.handle}`}
                      class="workspace-title"
                    />
                  )}
                </Show>
                <button
                  type="button"
                  class="row-close"
                  data-action="workspace-close"
                  aria-label={`Close workspace ${workspace.name}`}
                  title="Close workspace"
                  onClick={() =>
                    close({ kind: 'workspace', id: workspace.id, name: workspace.name })
                  }
                >
                  <CloseIcon />
                </button>
              </div>
              <ul class="workspace-tabs">
                <For each={workspace.tabs}>
                  {(tab) => (
                    <li class="side-tab-row">
                      <Show
                        when={edits('tab', tab.id)}
                        fallback={
                          <button
                            type="button"
                            class="side-tab"
                            classList={{
                              selected: tab.id === shown()?.tab,
                              'drop-target': over() === tab.id,
                            }}
                            data-side-tab={tab.id}
                            aria-keyshortcuts="F2"
                            onClick={() => select(tab.id)}
                            onDblClick={() => rename('tab', tab.id)}
                            onPointerDown={(event) =>
                              onLongPress(event, (x, y) =>
                                view.setPressMenu({
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
                            ref={(element) =>
                              dropTarget(element, {
                                accepts: (item) => item.kind === 'pane',
                                enter: () => setOver(tab.id),
                                leave: () =>
                                  setOver((current) => (current === tab.id ? null : current)),
                                drop: (item) => {
                                  if (item.kind === 'pane') drop(item.id, tab.id);
                                },
                              })
                            }
                          >
                            {tab.name}
                          </button>
                        }
                      >
                        {(target) => (
                          <RenameField
                            target={target()}
                            name={tab.name}
                            label={`Name of tab ${tab.handle}`}
                          />
                        )}
                      </Show>
                      <button
                        type="button"
                        class="row-close"
                        data-action="tab-close"
                        aria-label={`Close tab ${tab.name}`}
                        title="Close tab"
                        onClick={() => close({ kind: 'tab', id: tab.id, name: tab.name })}
                      >
                        <CloseIcon />
                      </button>
                    </li>
                  )}
                </For>
              </ul>
            </li>
          )}
        </For>
      </ul>
      <button type="button" class="new-workspace" onClick={create}>
        New workspace
      </button>
      {props.extra?.()}
    </nav>
  );
}

/** The folded left column: a rail with the Workspaces button. */
export function LeftRail() {
  const { view } = useApp();
  return (
    <nav class="left-rail" aria-label="Folded left column">
      <button
        type="button"
        class="rail-button"
        aria-label="Show Workspaces"
        title="Workspaces"
        onClick={() => view.setLeftOpen(true)}
      >
        <Icon name="workspaces" />
      </button>
    </nav>
  );
}

function CloseIcon() {
  return (
    <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" />
    </svg>
  );
}
