// The Timeline tab of the drawer: the turns of the agent in the pane that has the focus.
// The core builds the items from the transcript of the run (`timeline:<run>`) and gives
// older ones through `timeline.page`. The last message of the agent shows in full under the turns.
// The window follows the projection only while this tab shows,
// so the core reads the file for no one else.

import { RpcError, type TimelineItem } from '@neohar/transport';
import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { createProjectionStore, type ProjectionStore } from '../stores';
import { LastMessage } from './LastMessage';
import {
  focusedRun,
  groupByTurn,
  hasUsage,
  joinPages,
  lastMessage,
  LIVE_ITEMS,
  lineOf,
  usageText,
  windowText,
} from './model';

/** The items in one request of older ones. */
const PAGE = 100;

export function TimelinePanel() {
  const { stores, transport, windowId } = useApp();
  const pane = () => {
    const id = stores.windows.state.byKey[windowId]?.focused_pane;
    return id ? stores.panes.state.byKey[id] : undefined;
  };
  const run = createMemo(() => focusedRun(pane()));
  const agent = () => {
    const id = run();
    return id ? stores.agents.state.byKey[id] : undefined;
  };
  const [live, setLive] = createSignal<ProjectionStore<TimelineItem> | null>(null);
  const [older, setOlder] = createSignal<TimelineItem[]>([]);
  const [more, setMore] = createSignal<boolean | null>(null);
  const [message, setMessage] = createSignal<string | null>(null);

  // One subscription for the run in focus. It ends when the focus moves or the tab closes.
  createEffect(
    on(run, (id) => {
      setOlder([]);
      setMore(null);
      setMessage(null);
      const store = id ? createProjectionStore(transport, `timeline:${id}`) : null;
      setLive(store);
      onCleanup(() => store?.close());
    }),
  );

  const items = createMemo(() => joinPages(older(), live()?.rows() ?? []));
  const turns = createMemo(() => groupByTurn(items()));
  const last = createMemo(() => lastMessage(turns()[turns().length - 1]));
  // With fewer items than the projection can hold, no older item exists.
  const canLoadOlder = () => more() ?? (live()?.rows().length ?? 0) >= LIVE_ITEMS;

  const loadOlder = (): void => {
    const id = run();
    const first = items()[0];
    if (!id || !first) return;
    setMessage(null);
    transport
      .call('timeline.page', { run: id, before_item: first.id, limit: PAGE })
      .then((result) => {
        setOlder((previous) => [...result.items, ...previous]);
        setMore(result.more);
      })
      .catch((failure: unknown) => {
        setMessage(failure instanceof RpcError ? failure.message : 'The page did not load.');
      });
  };

  return (
    <div class="timeline-panel">
      <Show when={run()} fallback={<p class="drawer-empty">No agent is in focus.</p>}>
        <header class="timeline-head">
          <span class="timeline-agent">{agent() ? harnessName(agent()!.harness) : 'Agent'}</span>
          <Show when={agent() && hasUsage(agent()!.usage)}>
            <span class="timeline-usage" data-testid="timeline-usage">
              {usageText(agent()!.usage)}
            </span>
          </Show>
          <Show when={agent()?.rate_limit}>
            {(limit) => <span class="timeline-limit">{windowText(limit())}</span>}
          </Show>
        </header>
        <Show when={message()}>
          <p class="timeline-message" role="alert">
            {message()}
          </p>
        </Show>
        <Show
          when={items().length > 0}
          fallback={<p class="drawer-empty">Nothing is in the transcript yet.</p>}
        >
          <Show when={canLoadOlder()}>
            <button type="button" class="timeline-older" onClick={loadOlder}>
              Older items
            </button>
          </Show>
          <For each={turns()}>
            {(turn) => (
              <section class="timeline-turn" aria-label={`Turn ${turn.turn}`}>
                <h3 class="timeline-turn-title">Turn {turn.turn}</h3>
                <ul class="timeline-items">
                  <For each={turn.items}>
                    {(item) => {
                      const line = lineOf(item);
                      return (
                        <li class={`timeline-item timeline-${line.kind}`} data-item={item.id}>
                          <span class="timeline-label">{line.label}</span>
                          <Show when={line.status}>
                            <span class="timeline-status" data-status={line.status}>
                              {line.status}
                            </span>
                          </Show>
                          <Show
                            when={line.kind === 'output'}
                            fallback={<span class="timeline-text">{line.text}</span>}
                          >
                            <pre class="timeline-text">{line.text}</pre>
                          </Show>
                          <Show when={line.truncated}>
                            <span class="timeline-cut">cut</span>
                          </Show>
                        </li>
                      );
                    }}
                  </For>
                </ul>
              </section>
            )}
          </For>
          <Show when={run() !== undefined && last() !== undefined}>
            <LastMessage run={run()!} item={last()!} />
          </Show>
        </Show>
      </Show>
    </div>
  );
}
