// The board: the row under the command line with a strip for each agent run, in the
// order of the core (`agents_for:<window>`). The page does not sort the strips, so a strip keeps its
// place when its state changes. A drag of a strip onto another strip sends `agent.move` with the
// place of the other strip, and the core changes the order for every window. The board has a fixed
// height of two lines, or one when it is narrow, because a change of its height resizes every pane
// and its PTY: a new agent never changes it. A `ResizeObserver` gives the width of the board to
// `fitStrips`, and the runs that do not fit go behind the `+N` cell, which opens the list of every
// agent. The cell is amber when a run behind it waits. The attention line of the command line
// still counts every run that waits. A right click on a strip opens its menu: show the pane, or show
// the worktree of its folder in the Git view.

import type { AgentValue } from '@neohar/transport';
import {
  type CSSProperties,
  type KeyboardEvent,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { badgeOf, workOf } from '../agents/badge';
import { useApp } from '../app/context';
import { dragged, dropTarget, pressToDrag, type Dragged } from '../app/drag';
import { showPane } from '../app/show';
import { shallowEqual, useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { hasUsage, usageText } from '../timeline/model';
import { BOARD_LIST, BoardList } from './BoardList';
import { fitStrips, waitsBehind } from './fit';
import { Strip } from './Strip';

/** The probe that measures one character of the chrome font: 20 zeros. */
const PROBE = '0'.repeat(20);

export function Board() {
  const { stores, transport, run, report, windowId, view, form: shellForm } = useApp();
  const runs = useRows(stores.agents);
  const [width, setWidth] = useState(0);
  const board = useRef<HTMLElement>(null);
  const probe = useRef<HTMLSpanElement>(null);

  // The width of the board in characters. The probe changes its size with the font of the
  // settings, so a new font fits the strips again too.
  useLayoutEffect(() => {
    const observer = new ResizeObserver(() => {
      if (!board.current || !probe.current) return;
      const char = probe.current.getBoundingClientRect().width / PROBE.length;
      setWidth(char > 0 ? board.current.clientWidth / char : 0);
    });
    if (board.current) observer.observe(board.current);
    if (probe.current) observer.observe(probe.current);
    return () => observer.disconnect();
  }, []);

  const fit = fitStrips(runs.length, width);
  const shown = runs.slice(0, fit.shown);
  const waiting = waitsBehind(
    runs.map((run) => badgeOf(run).waiting),
    fit.shown,
  );
  const form: 'two' | 'narrow' = fit.narrow ? 'narrow' : 'two';

  // A drag of a strip: the strip under the pointer shows a bar on the side where the run lands.
  const [over, setOver] = useState<string | null>(null);
  const moving = useValue(dragged);
  /** The place of the run that the person drags, or -1. */
  const from = moving?.kind === 'agent' ? runs.findIndex((run) => run.id === moving.id) : -1;
  const lifted = (id: string): boolean => moving?.kind === 'agent' && moving.id === id;
  const drop = (item: Dragged, index: number): void => {
    setOver(null);
    if (item.kind === 'agent') run('agent.move', { agent: item.id, index });
  };

  // The panes and the window are hot projections: the board reads only the titles of the panes
  // of its runs and the focused pane, so that another change of them does not draw it again.
  const titles = useSelect(
    stores.panes,
    (state) => {
      const picked: Record<string, string> = {};
      for (const run of runs) {
        const pane = run.pane ? state.byKey[run.pane] : undefined;
        if (run.pane && pane) picked[run.pane] = pane.title;
      }
      return picked;
    },
    shallowEqual,
  );
  const focusedPane = useSelect(stores.windows, (state) => state.byKey[windowId]?.focused_pane);
  const work = (run: AgentValue): string => {
    const title = run.pane ? titles[run.pane] : undefined;
    return workOf(run, title === undefined ? undefined : { title });
  };
  const tooltip = (run: AgentValue): string => {
    const usage = hasUsage(run.usage) ? `\n${usageText(run.usage)}` : '';
    return `${badgeOf(run).tooltip}${usage}`;
  };
  const focused = (run: AgentValue): boolean => !!run.pane && run.pane === focusedPane;
  const show = useCallback(
    (pane: string): void => {
      showPane(transport, windowId, pane).catch(report);
    },
    [transport, windowId, report],
  );

  // The menu of a strip is a popover in the top layer: a click elsewhere closes it.
  const [menu, setMenu] = useState<{ pane: string; x: number; y: number } | null>(null);
  const menuBox = useRef<HTMLDivElement | null>(null);
  const menuFolder = useSelect(stores.panes, (state) =>
    menu ? (state.byKey[menu.pane]?.cwd ?? undefined) : undefined,
  );
  const folderOf = (pane: string): string | undefined =>
    stores.panes.get().byKey[pane]?.cwd ?? undefined;
  useLayoutEffect(() => {
    const box = menuBox.current;
    if (!menu || !box) return;
    if (!box.matches(':popover-open')) box.showPopover();
    box.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [menu]);
  const attachMenu = useCallback((element: HTMLDivElement | null) => {
    menuBox.current = element;
    if (!element) return;
    const toggle = (event: Event): void => {
      if ((event as ToggleEvent).newState === 'closed') setMenu(null);
    };
    element.addEventListener('toggle', toggle);
    return () => {
      element.removeEventListener('toggle', toggle);
      menuBox.current = null;
    };
  }, []);
  const closeMenu = (): void => {
    if (menuBox.current?.matches(':popover-open')) menuBox.current.hidePopover();
    setMenu(null);
  };
  const onMenuKey = (event: KeyboardEvent): void => {
    const items = [...(menuBox.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault();
      closeMenu();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      items[(at + step + items.length) % items.length]?.focus();
    }
  };

  return (
    <section
      className={cn('board', { 'one-line': fit.narrow })}
      style={{ '--strip': String(fit.strip) } as CSSProperties}
      aria-label="Board"
      ref={board}
    >
      <span className="char-probe" aria-hidden="true" ref={probe}>
        {PROBE}
      </span>
      {runs.length > 0 ? (
        <>
          <ul className="strips" aria-label="Agents">
            {shown.map((run, index) => (
              <li
                key={run.id}
                className={
                  cn({
                    lifted: lifted(run.id),
                    'drop-before': over === run.id && from > index,
                    'drop-after': over === run.id && from >= 0 && from < index,
                  }) || undefined
                }
                ref={(element) => {
                  if (!element) return;
                  dropTarget(element, {
                    accepts: (item) => item.kind === 'agent' && item.id !== run.id,
                    enter: () => setOver(run.id),
                    leave: () => setOver((current) => (current === run.id ? null : current)),
                    drop: (item) => drop(item, index),
                  });
                }}
                onPointerDown={(event) =>
                  pressToDrag(event.nativeEvent, { kind: 'agent', id: run.id })
                }
                onContextMenu={(event) => {
                  if (shellForm === 'phone' || !run.pane) return;
                  event.preventDefault();
                  setMenu({ pane: run.pane, x: event.clientX, y: event.clientY });
                }}
              >
                <Strip
                  run={run}
                  work={work(run)}
                  tooltip={tooltip(run)}
                  focused={focused(run)}
                  form={form}
                  onShow={show}
                />
              </li>
            ))}
          </ul>
          {fit.hidden > 0 ? (
            <button
              type="button"
              className={cn('board-more', { waiting })}
              popoverTarget={BOARD_LIST}
              aria-label={`${fit.hidden} more agents${waiting ? ', one waits for you' : ''}`}
              title="Every agent"
            >
              +{fit.hidden}
            </button>
          ) : null}
        </>
      ) : (
        <p className="board-empty">No agent runs. Start one in a pane.</p>
      )}
      <BoardList runs={runs} work={work} tooltip={tooltip} focused={focused} onShow={show} />
      {menu ? (
        <div
          className="files-menu"
          role="menu"
          popover="auto"
          aria-label="Actions of the agent"
          ref={attachMenu}
          style={{ left: `${menu.x}px`, top: `${menu.y}px` }}
          onKeyDown={onMenuKey}
        >
          <button
            type="button"
            role="menuitem"
            className="files-menu-item"
            tabIndex={-1}
            onClick={() => {
              const pane = menu.pane;
              closeMenu();
              show(pane);
            }}
          >
            Show the pane
          </button>
          <button
            type="button"
            role="menuitem"
            className="files-menu-item"
            tabIndex={-1}
            disabled={!menuFolder}
            onClick={() => {
              const folder = folderOf(menu.pane);
              closeMenu();
              if (folder) view.openGit(folder);
            }}
          >
            Show in Git view
          </button>
          <button
            type="button"
            role="menuitem"
            className="files-menu-item"
            tabIndex={-1}
            disabled={!menuFolder}
            onClick={() => {
              const folder = folderOf(menu.pane);
              closeMenu();
              if (folder) view.openWorkflows(folder);
            }}
          >
            Execute a workflow here
          </button>
        </div>
      ) : null}
    </section>
  );
}
