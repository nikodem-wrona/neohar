// The tabs of the workspace that the window shows. A click selects a tab through `tab.select`,
// and the window follows the `windows` projection. A drag of a tab onto another tab sends
// `tab.move`, and a drag of a pane onto a tab sends `pane.move`. The bar keeps no order of its
// own: the tabs change place when the projection says so. The cross of a tab, a click with the
// middle button and Delete on a tab close it. With `ui.show_handles`, the handle of a tab
// shows before its name (a CSS `::before` of `data-handle`), so that the text of the tab
// stays its name. A double click
// or F2 on a tab opens a field for a new name. A tab may hold no field, so the field lies
// over the tab and is not in the list of the tabs. On a phone, a long press on a tab opens its
// menu: Rename, Close and a move to either side.

import type { TabValue } from '@neohar/transport';
import {
  type CSSProperties,
  type Dispatch,
  memo,
  type SetStateAction,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { showHandles } from '../app/preferences';
import { dragged, dropTarget, pressToDrag, type Dragged } from '../app/drag';
import { onLongPress } from '../app/press';
import { useLatest, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { requestClose } from './close';
import { RenameField } from './RenameField';

export function TabBar() {
  const { stores, run, windowId, view } = useApp();
  const [over, setOver] = useState<string | null>(null);
  const handles = useSelect(stores.settings, (state) => showHandles(state.byKey));
  const bar = useRef<HTMLDivElement>(null);
  const shownTab = useSelect(stores.windows, (state) => state.byKey[windowId]?.tab);
  const workspaceId = useSelect(stores.windows, (state) => state.byKey[windowId]?.workspace);
  const workspace = useSelect(stores.tree, (state) =>
    workspaceId ? state.byKey[workspaceId] : undefined,
  );
  const renaming = useValue(view.renaming);
  const lifted = useValue(dragged);
  const create = () => {
    const id = workspace?.id;
    if (id) run('tab.create', { workspace: id });
  };
  const tabOf = (id: string) => workspace?.tabs.find((candidate) => candidate.id === id);
  /** The tab whose name the person edits in this bar, while the bar shows it. */
  const edited = ((): string | undefined => {
    const target = renaming;
    if (target?.kind !== 'tab' || target.place !== 'tabbar') return undefined;
    return tabOf(target.id) ? target.id : undefined;
  })();
  /** The place of the field: over the tab, and in the bar when the bar cuts the tab off. */
  const [place, setPlace] = useState<CSSProperties>({});
  // The bar measures the tab once for each field, before the paint.
  useLayoutEffect(() => {
    if (edited === undefined) return;
    const room = bar.current;
    const element = room?.querySelector<HTMLElement>(`[data-tab="${CSS.escape(edited)}"]`);
    if (!room || !element) {
      setPlace({});
      return;
    }
    const box = element.getBoundingClientRect();
    const within = room.getBoundingClientRect();
    const left = Math.max(0, Math.min(box.left - within.left, within.width - box.width));
    setPlace({ left: `${left}px`, width: `${box.width}px` });
  }, [edited]);
  const tabs = workspace?.tabs ?? [];
  return (
    <div className="tabbar" ref={bar}>
      <div className="tablist" role="tablist" aria-label="Tabs">
        {tabs.map((tab, index) => (
          <Tab
            key={tab.id}
            tab={tab}
            index={index}
            count={tabs.length}
            selected={tab.id === shownTab}
            target={over === tab.id}
            lifted={isLifted(lifted, tab.id)}
            handle={handles ? tab.handle : undefined}
            setOver={setOver}
          />
        ))}
      </div>
      {edited !== undefined ? (
        <RenameField
          key={edited}
          target={{ kind: 'tab', id: edited, place: 'tabbar' }}
          name={tabOf(edited)?.name ?? ''}
          label={`Name of tab ${tabOf(edited)?.handle ?? ''}`}
          className="tab-rename"
          style={place}
        />
      ) : null}
      <button
        type="button"
        className="tab-new"
        aria-label="New tab"
        title="New tab"
        disabled={!workspace}
        onClick={create}
      >
        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
          <path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth="1.5" fill="none" />
        </svg>
      </button>
    </div>
  );
}

function isLifted(item: Dragged | null, id: string): boolean {
  return item?.kind === 'tab' && item.id === id;
}

/** One tab of the bar. It renders again only when its row or its marks change. */
const Tab = memo(function Tab(props: {
  tab: TabValue;
  index: number;
  /** How many tabs the bar has. */
  count: number;
  selected: boolean;
  /** A dragged item is over the tab. */
  target: boolean;
  lifted: boolean;
  /** The handle that shows before the name, with `ui.show_handles`. */
  handle: string | undefined;
  setOver: Dispatch<SetStateAction<string | null>>;
}) {
  const { transport, run, report, windowId, view } = useApp();
  const { tab, setOver } = props;
  // The place of the tab when a long press ends, which is later than this render.
  const latest = useLatest({ index: props.index, count: props.count });
  const select = (id: string) => run('tab.select', { tab: id, window: windowId });
  const rename = (id: string): void => {
    view.renaming.set({ kind: 'tab', id, place: 'tabbar' });
  };
  const close = (target: { id: string; name: string }): void => {
    requestClose(transport, { kind: 'tab', id: target.id, name: target.name })
      .then((closing) => view.closing.set(closing))
      .catch(report);
  };
  const drop = (item: Dragged, target: string, index: number): void => {
    setOver(null);
    if (item.kind === 'tab') run('tab.move', { tab: item.id, index });
    else if (item.kind === 'pane') run('pane.move', { pane: item.id, tab: target });
  };
  return (
    <button
      type="button"
      role="tab"
      className={cn('tab', {
        selected: props.selected,
        'drop-target': props.target,
        lifted: props.lifted,
      })}
      aria-selected={props.selected}
      data-tab={tab.id}
      data-handle={props.handle}
      aria-keyshortcuts="Delete F2"
      onClick={() => select(tab.id)}
      onDoubleClick={() => rename(tab.id)}
      onAuxClick={(event) => {
        if (event.button !== 1) return;
        event.preventDefault();
        close(tab);
      }}
      onKeyDown={(event) => {
        if (event.key === 'F2') {
          event.preventDefault();
          rename(tab.id);
          return;
        }
        if (event.key !== 'Delete' && event.key !== 'Backspace') return;
        event.preventDefault();
        close(tab);
      }}
      ref={(element) => {
        if (!element) return;
        dropTarget(element, {
          accepts: (item) => (item.kind === 'tab' || item.kind === 'pane') && item.id !== tab.id,
          enter: () => setOver(tab.id),
          leave: () => setOver((current) => (current === tab.id ? null : current)),
          drop: (item) => drop(item, tab.id, latest.current.index),
        });
      }}
      onPointerDown={(event) => {
        pressToDrag(event.nativeEvent, { kind: 'tab', id: tab.id });
        onLongPress(event.nativeEvent, (x, y) => {
          const count = latest.current.count;
          const at = latest.current.index;
          view.pressMenu.set({
            x,
            y,
            title: tab.name,
            items: [
              { label: 'Rename', run: () => rename(tab.id) },
              ...(at > 0
                ? [
                    {
                      label: 'Move left',
                      run: () => run('tab.move', { tab: tab.id, index: at - 1 }),
                    },
                  ]
                : []),
              ...(at < count - 1
                ? [
                    {
                      label: 'Move right',
                      run: () => run('tab.move', { tab: tab.id, index: at + 1 }),
                    },
                  ]
                : []),
              { label: 'Close', run: () => close(tab), danger: true },
            ],
          });
        });
      }}
    >
      <span className="tab-name">{tab.name}</span>
      {/* A pointer target only: a button in a tab is not allowed, and Delete closes the
          tab from the keyboard. */}
      <span
        className="tab-close"
        aria-hidden="true"
        title="Close tab"
        onClick={(event) => {
          event.stopPropagation();
          close(tab);
        }}
      >
        <svg viewBox="0 0 16 16" width="12" height="12">
          <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
        </svg>
      </span>
    </button>
  );
});
