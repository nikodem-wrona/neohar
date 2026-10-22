// The inbox: the cards of the `interactions` projection, grouped by run, in the order that the
// core gives. A run that counts in `attention` without a card, because it is in `error`
// or `rate_limited` or its turn ended while nobody looked, has a line with its state
// and Open. The window keeps no state of its
// own about them. `⌘⇧A` asks the core which interaction is next and moves the focus to its card.
// The head of a group names the pane by its handle with `ui.show_handles`, else by its title.

import { createEffect, For, Show } from 'solid-js';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { paneName, showHandles } from '../app/preferences';
import { showPane } from '../app/show';
import { Card } from '../cards/Card';
import { groupByRun, replyState, type Trouble, troubleTitle } from './group';
import { Reply } from './Reply';

export function Inbox() {
  const { stores, view } = useApp();
  const cards = new Map<string, HTMLElement>();
  const groups = () => groupByRun(stores.interactions.rows(), stores.attention.rows());
  const harnessOf = (run: string | null) => {
    const agent = run ? stores.agents.state.byKey[run] : undefined;
    return agent ? harnessName(agent.harness) : 'the agent';
  };
  const paneOf = (run: string | null) => {
    const agent = run ? stores.agents.state.byKey[run] : undefined;
    const pane = agent?.pane ? stores.panes.state.byKey[agent.pane] : undefined;
    return paneName(pane, showHandles(stores.settings.state.byKey)) ?? null;
  };

  // The card takes the focus once it is in the page, and the request ends.
  createEffect(() => {
    const wanted = view.focusedCard();
    if (!wanted) return;
    const card = cards.get(wanted);
    if (!card) return;
    card.focus();
    card.scrollIntoView({ block: 'nearest' });
    view.focusCard(null);
  });

  return (
    <div class="inbox">
      <Show
        when={groups().length > 0}
        fallback={<p class="drawer-empty">Nothing waits for an answer.</p>}
      >
        <For each={groups()}>
          {(group) => (
            <section class="inbox-group" aria-label={harnessOf(group.run)}>
              <h2 class="inbox-run">
                {harnessOf(group.run)}
                <Show when={paneOf(group.run)}>
                  <span class="inbox-pane"> · {paneOf(group.run)}</span>
                </Show>
              </h2>
              <Show when={group.trouble}>
                {(trouble) => (
                  <RunState run={group.run} trouble={trouble()} harness={harnessOf(group.run)} />
                )}
              </Show>
              <For each={group.interactions}>
                {(interaction) => (
                  <Card
                    interaction={interaction}
                    harness={harnessOf(group.run)}
                    ref={(element) => cards.set(interaction.id, element)}
                  />
                )}
              </For>
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}

/**
 * The line of a run that counts in the inbox without a card: what is wrong, since when,
 * and Open, which shows the pane, in any tab, so the person sees what the harness shows.
 */
function RunState(props: { run: string | null; trouble: Trouble; harness: string }) {
  const { stores, transport, report, windowId, form } = useApp();
  const agent = () => (props.run ? stores.agents.state.byKey[props.run] : undefined);
  const since = () => (props.run ? stores.attention.state.byKey[props.run]?.since : undefined);
  const title = () => troubleTitle(props.trouble);
  const text = () => {
    switch (props.trouble) {
      case 'error':
        return `${props.harness} stopped with an error. Open the pane to see it.`;
      case 'rate_limited':
        return `${props.harness} waits for its limit to reset.`;
      case 'turn_ended':
        return `${props.harness} finished its turn. Open the pane to read it and reply.`;
    }
  };
  return (
    <article
      class="card card-run-state"
      data-state={props.trouble}
      tabindex="0"
      aria-label={`${props.harness}: ${title()}`}
    >
      <header class="card-head">
        <h3 class="card-title">{title()}</h3>
      </header>
      <p class="card-text">{text()}</p>
      <Show when={since()}>
        {(at) => (
          <p class="card-cwd">
            <span class="card-label">Since</span> <time datetime={at()}>{at()}</time>
          </p>
        )}
      </Show>
      {/* A phone answers a turn that ended in the Inbox. */}
      <Show
        when={
          form === 'phone' && props.trouble === 'turn_ended' && replyState(agent()) === 'ready'
            ? props.run
            : null
        }
      >
        {(run) => <Reply run={run()} harness={props.harness} />}
      </Show>
      <Show when={agent()?.pane}>
        {(pane) => (
          <div class="card-actions" role="group" aria-label={title()}>
            <button
              type="button"
              class="card-button"
              onClick={() => void showPane(transport, windowId, pane()).catch(report)}
            >
              Open
            </button>
          </div>
        )}
      </Show>
    </article>
  );
}
