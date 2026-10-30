// The board: the row under the command line with a strip for each agent run, in the
// order of the core (`agents_for:<window>`). The page does not sort the strips, so a strip keeps its
// place when its state changes. A drag of a strip onto another strip sends `agent.move` with the
// place of the other strip, and the core changes the order for every window. The board has a fixed
// height of two lines, or one when it is narrow, because a change of its height resizes every pane
// and its PTY: a new agent never changes it. A `ResizeObserver` gives the width of the board to
// `fitStrips`, and the runs that do not fit go behind the `+N` cell, which opens the list of every
// agent. The cell is amber when a run behind it waits. The attention line of the command line
// still counts every run that waits. A right click on a strip opens its menu: show the pane, or show
// the worktree of its folder in the Git view, in a Context Menu of shadcn/ui. The list behind the
// `+N` cell is a Popover.

import type { AgentValue } from '@neohar/transport';
import {
  type CSSProperties,
  type ReactElement,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { badgeOf, workOf } from '../agents/badge';
import { useApp } from '../app/context';
import { dragged, dropTarget, pressToDrag, type Dragged } from '../app/drag';
import { useLayer } from '../app/layer';
import { showPane } from '../app/show';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '../components/ui/context-menu';
import { Popover, PopoverTrigger } from '../components/ui/popover';
import { shallowEqual, useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { hasUsage, usageText } from '../timeline/model';
import { BoardList } from './BoardList';
import { fitStrips, waitsBehind } from './fit';
import { Strip } from './Strip';
import { Tip } from '../shell/Tip';

/** The probe that measures one character of the chrome font: 20 zeros. */
const PROBE = '0'.repeat(20);

export function Board() {
  const { stores, transport, run, report, windowId, form: shellForm } = useApp();
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

  // The list of every agent behind the `+N` cell. A click on a row shows the pane and closes it.
  const [listOpen, setListOpen] = useState(false);
  useLayer(listOpen);
  const showFromList = useCallback(
    (pane: string): void => {
      setListOpen(false);
      show(pane);
    },
    [show],
  );

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
              <StripMenu
                key={run.id}
                pane={run.pane ?? null}
                disabled={shellForm === 'phone' || !run.pane}
                onShow={show}
              >
                <li
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
              </StripMenu>
            ))}
          </ul>
          {fit.hidden > 0 ? (
            <Popover open={listOpen} onOpenChange={setListOpen}>
              <Tip text="Every agent" hidden={listOpen}>
                <PopoverTrigger asChild>
                  <button
                    type="button"
                    className={cn('board-more', { waiting })}
                    aria-label={`${fit.hidden} more agents${waiting ? ', one waits for you' : ''}`}
                  >
                    +{fit.hidden}
                  </button>
                </PopoverTrigger>
              </Tip>
              <BoardList
                runs={runs}
                work={work}
                tooltip={tooltip}
                focused={focused}
                onShow={showFromList}
              />
            </Popover>
          ) : null}
        </>
      ) : (
        <p className="board-empty">No agent runs. Start one in a pane.</p>
      )}
    </section>
  );
}

/**
 * The menu of a strip: a right click opens it at the pointer. It shows the pane, or shows the
 * worktree of its folder in the Git view. A phone has no menu of a strip.
 */
function StripMenu(props: {
  pane: string | null;
  disabled: boolean;
  onShow: (pane: string) => void;
  children: ReactElement;
}) {
  const [open, setOpen] = useState(false);
  useLayer(open);
  return (
    <ContextMenu onOpenChange={setOpen}>
      <ContextMenuTrigger asChild disabled={props.disabled}>
        {props.children}
      </ContextMenuTrigger>
      <ContextMenuContent
        aria-label="Actions of the agent"
        className="min-w-[24ch] rounded-(--r-md) p-1"
      >
        {props.pane ? <StripMenuItems pane={props.pane} onShow={props.onShow} /> : null}
      </ContextMenuContent>
    </ContextMenu>
  );
}

/** The items of the menu of a strip. They read the folder of the pane while the menu is open. */
function StripMenuItems(props: { pane: string; onShow: (pane: string) => void }) {
  const { stores, view } = useApp();
  const folder = useSelect(stores.panes, (state) => state.byKey[props.pane]?.cwd ?? undefined);
  const item = 'px-2 py-1 text-sm';
  return (
    <>
      <ContextMenuItem className={item} onSelect={() => props.onShow(props.pane)}>
        Show the pane
      </ContextMenuItem>
      <ContextMenuItem
        className={item}
        disabled={!folder}
        onSelect={() => {
          if (folder) view.openGit(folder);
        }}
      >
        Show in Git view
      </ContextMenuItem>
      <ContextMenuItem
        className={item}
        disabled={!folder}
        onSelect={() => {
          if (folder) view.openWorkflows(folder);
        }}
      >
        Execute a workflow here
      </ContextMenuItem>
    </>
  );
}
