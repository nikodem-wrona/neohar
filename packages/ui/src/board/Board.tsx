// The board: the row under the command line with a strip for each agent run, in the
// order of the core (`agents_for:<window>`). The page does not sort the strips, so a strip keeps its
// place when its state changes. A drag of a strip onto another strip sends `agent.move` with the
// place of the other strip, and the core changes the order for every window. The board has a fixed
// height of two lines, or one when it is narrow, because a change of its height resizes every pane
// and its PTY: a new agent never changes it. A `ResizeObserver` gives the width of the board to
// `fitStrips`, and the runs that do not fit go behind the `+N` cell, which opens the list of every
// agent. The cell is amber when a run behind it waits. The attention line of the command line
// still counts every run that waits.

import type { AgentValue } from '@neohar/transport';
import { createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { badgeOf, workOf } from '../agents/badge';
import { useApp } from '../app/context';
import { dragged, dropTarget, pressToDrag, type Dragged } from '../app/drag';
import { showPane } from '../app/show';
import { hasUsage, usageText } from '../timeline/model';
import { BOARD_LIST, BoardList } from './BoardList';
import { fitStrips, waitsBehind } from './fit';
import { Strip } from './Strip';

/** The probe that measures one character of the chrome font: 20 zeros. */
const PROBE = '0'.repeat(20);

export function Board() {
  const { stores, transport, run, report, windowId } = useApp();
  const runs = () => stores.agents.rows();
  const [width, setWidth] = createSignal(0);
  let board: HTMLElement | undefined;
  let probe: HTMLSpanElement | undefined;

  // The width of the board in characters. The probe changes its size with the font of the
  // settings, so a new font fits the strips again too.
  onMount(() => {
    const observer = new ResizeObserver(() => {
      if (!board || !probe) return;
      const char = probe.getBoundingClientRect().width / PROBE.length;
      setWidth(char > 0 ? board.clientWidth / char : 0);
    });
    if (board) observer.observe(board);
    if (probe) observer.observe(probe);
    onCleanup(() => observer.disconnect());
  });

  const fit = createMemo(() => fitStrips(runs().length, width()));
  const shown = () => runs().slice(0, fit().shown);
  const waiting = () =>
    waitsBehind(
      runs().map((run) => badgeOf(run).waiting),
      fit().shown,
    );
  const form = (): 'two' | 'narrow' => (fit().narrow ? 'narrow' : 'two');

  // A drag of a strip: the strip under the pointer shows a bar on the side where the run lands.
  const [over, setOver] = createSignal<string | null>(null);
  /** The place of the run that the person drags, or -1. */
  const from = (): number => {
    const item = dragged();
    return item?.kind === 'agent' ? runs().findIndex((run) => run.id === item.id) : -1;
  };
  const lifted = (id: string): boolean => {
    const item = dragged();
    return item?.kind === 'agent' && item.id === id;
  };
  const drop = (item: Dragged, index: number): void => {
    setOver(null);
    if (item.kind === 'agent') run('agent.move', { agent: item.id, index });
  };

  const work = (run: AgentValue): string =>
    workOf(
      run,
      run.task ? stores.tasks.state.byKey[run.task] : undefined,
      run.pane ? stores.panes.state.byKey[run.pane] : undefined,
    );
  const tooltip = (run: AgentValue): string => {
    const usage = hasUsage(run.usage) ? `\n${usageText(run.usage)}` : '';
    return `${badgeOf(run).tooltip}${usage}`;
  };
  const focused = (run: AgentValue): boolean =>
    !!run.pane && run.pane === stores.windows.state.byKey[windowId]?.focused_pane;
  const show = (pane: string): void => {
    showPane(transport, windowId, pane).catch(report);
  };

  return (
    <section
      class="board"
      classList={{ 'one-line': fit().narrow }}
      style={{ '--strip': String(fit().strip) }}
      aria-label="Board"
      ref={(element) => (board = element)}
    >
      <span class="char-probe" aria-hidden="true" ref={(element) => (probe = element)}>
        {PROBE}
      </span>
      <Show
        when={runs().length > 0}
        fallback={<p class="board-empty">No agent runs. Start one in a pane or with a task.</p>}
      >
        <ul class="strips" aria-label="Agents">
          <For each={shown()}>
            {(run, index) => (
              <li
                classList={{
                  lifted: lifted(run.id),
                  'drop-before': over() === run.id && from() > index(),
                  'drop-after': over() === run.id && from() >= 0 && from() < index(),
                }}
                ref={(element) =>
                  dropTarget(element, {
                    accepts: (item) => item.kind === 'agent' && item.id !== run.id,
                    enter: () => setOver(run.id),
                    leave: () => setOver((current) => (current === run.id ? null : current)),
                    drop: (item) => drop(item, index()),
                  })
                }
                onPointerDown={(event) => pressToDrag(event, { kind: 'agent', id: run.id })}
              >
                <Strip
                  run={run}
                  work={work(run)}
                  tooltip={tooltip(run)}
                  focused={focused(run)}
                  form={form()}
                  onShow={show}
                />
              </li>
            )}
          </For>
        </ul>
        <Show when={fit().hidden > 0}>
          <button
            type="button"
            class="board-more"
            classList={{ waiting: waiting() }}
            popovertarget={BOARD_LIST}
            aria-label={`${fit().hidden} more agents${waiting() ? ', one waits for you' : ''}`}
            title="Every agent"
          >
            +{fit().hidden}
          </button>
        </Show>
      </Show>
      <BoardList runs={runs()} work={work} tooltip={tooltip} focused={focused} onShow={show} />
    </section>
  );
}
