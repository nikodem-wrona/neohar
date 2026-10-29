// The chat of the agent in a pane, in place of its terminal. The harness keeps running in the
// pane: this is a second view of the same run, and the toggle of the pane shows the terminal again
// at any time. The view follows `timeline:<run>`, asks `timeline.texts` for the full text of the
// prompts and the messages, and shows the cards of the interactions that wait. A tool call is one
// quiet line, and its output stays in the terminal and the Timeline tab. The surface of the
// pane is not there, so the pane has no focus for the gate: the core holds a question and the card
// answers it. A switch to the terminal gives the dialog back to the harness.

import { RpcError, type TimelineItem, type TimelineText } from '@neohar/transport';
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Index,
  type JSX,
  Match,
  on,
  onCleanup,
  Show,
  Switch,
} from 'solid-js';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { Card } from '../cards/Card';
import { replyState } from '../drawer/group';
import { RichHtml } from '../rich/RichHtml';
import { createProjectionStore, type ProjectionStore } from '../stores';
import { joinPages, LIVE_ITEMS, lineOf } from '../timeline/model';
import { Composer } from './Composer';
import { groupMessages, missingTexts, textKey, workingText } from './model';

/** The items in one request of older ones. */
const PAGE = 100;
/** How near the end, in pixels, the list must be to follow new messages. */
const FOLLOW_PX = 48;

export function ChatView(props: { pane: string; run: string; focused: boolean }) {
  const { stores, transport } = useApp();
  const agent = () => stores.agents.state.byKey[props.run];
  const harness = () => {
    const value = agent();
    return value ? harnessName(value.harness) : 'the agent';
  };
  const [live, setLive] = createSignal<ProjectionStore<TimelineItem> | null>(null);
  const [older, setOlder] = createSignal<TimelineItem[]>([]);
  const [more, setMore] = createSignal<boolean | null>(null);
  const [texts, setTexts] = createSignal<ReadonlyMap<string, TimelineText>>(new Map());
  const [message, setMessage] = createSignal<string | null>(null);
  /** The keys of the texts that went out in a request. */
  let asked = new Set<string>();
  let list: HTMLDivElement | undefined;
  let follow = true;

  createEffect(
    on(
      () => props.run,
      (run) => {
        setOlder([]);
        setMore(null);
        setTexts(new Map());
        setMessage(null);
        asked = new Set();
        follow = true;
        const store = createProjectionStore(transport, `timeline:${run}`);
        setLive(store);
        onCleanup(() => store.close());
      },
    ),
  );

  const items = createMemo(() => joinPages(older(), live()?.rows() ?? []));
  const groups = createMemo(() => groupMessages(items()));
  const canLoadOlder = () => more() ?? (live()?.rows().length ?? 0) >= LIVE_ITEMS;

  // The full texts that the view does not have yet, in one call for each change of the items.
  createEffect(() => {
    const run = props.run;
    const have = texts();
    const missing = missingTexts(items(), (key) => have.has(key) || asked.has(key));
    if (missing.length === 0) return;
    const keys = new Map(missing.map((item) => [item.id, textKey(item) ?? item.id]));
    for (const key of keys.values()) asked.add(key);
    transport.call('timeline.texts', { run, items: [...keys.keys()] }).then(
      (result) => {
        if (run !== props.run) return;
        setTexts((before) => {
          const next = new Map(before);
          for (const text of result.texts) {
            const key = keys.get(text.item);
            if (key) next.set(key, text);
          }
          return next;
        });
      },
      (failure: unknown) => {
        // The next change of the items asks again.
        for (const key of keys.values()) asked.delete(key);
        setMessage(failure instanceof RpcError ? failure.message : 'The messages did not load.');
      },
    );
  });

  // The list stays at its end while the person has not scrolled up.
  createEffect(
    on([items, texts], () => {
      if (follow && list) list.scrollTop = list.scrollHeight;
    }),
  );

  const loadOlder = (): void => {
    const first = items()[0];
    if (!first) return;
    const run = props.run;
    follow = false;
    transport
      .call('timeline.page', { run, before_item: first.id, limit: PAGE })
      .then((result) => {
        if (run !== props.run) return;
        setOlder((before) => [...result.items, ...before]);
        setMore(result.more);
      })
      .catch((failure: unknown) => {
        setMessage(failure instanceof RpcError ? failure.message : 'The page did not load.');
      });
  };

  const waiting = () =>
    (agent()?.facets.pending ?? [])
      .map((id) => stores.interactions.state.byKey[id])
      .filter((interaction) => interaction !== undefined);

  const full = (item: TimelineItem): TimelineText | undefined => {
    const key = textKey(item);
    return key === null ? undefined : texts().get(key);
  };

  /** One prompt, one message or one tool line of the agent. */
  const Item = (itemProps: { item: TimelineItem; from: 'you' | 'agent' }) => {
    // The store changes an item in place, for example the status of a tool call.
    const line = () => lineOf(itemProps.item);
    const kind = () => line().kind;
    return (
      <Switch
        fallback={
          <p class={`chat-line chat-${kind()}`} data-item={itemProps.item.id}>
            <span class="chat-label">{line().label}</span>
            <span class="chat-text">{line().text}</span>
            <Show when={line().status !== 'done' && line().status}>
              <span class="chat-status" data-status={line().status}>
                {line().status}
              </span>
            </Show>
          </p>
        }
      >
        <Match when={kind() === 'prompt' || kind() === 'assistant'}>
          <div class="chat-bubble" data-from={itemProps.from} data-item={itemProps.item.id}>
            <Show when={full(itemProps.item)} fallback={<p class="chat-preview">{line().text}</p>}>
              {(text) => (
                <>
                  <RichHtml html={text().html} />
                  <Show when={text().truncated}>
                    <p class="chat-cut">The message is cut at the size limit.</p>
                  </Show>
                </>
              )}
            </Show>
          </div>
        </Match>
        {/* The output of a tool stays in the terminal and the Timeline tab. */}
        <Match when={kind() === 'output'}>{null}</Match>
      </Switch>
    );
  };

  /** The name of the harness over its messages, and its mark beside them. */
  const Agent = (agentProps: { children: JSX.Element }) => (
    <section class="chat-agent" aria-label={harness()}>
      <span class="chat-name">{harness()}</span>
      <Avatar harness={agent()?.harness} />
      <div class="chat-agent-items">{agentProps.children}</div>
    </section>
  );

  return (
    <div class="chat" data-run={props.run} data-harness={agent()?.harness}>
      <div
        class="chat-list"
        ref={(element) => (list = element)}
        onScroll={(event) => {
          const box = event.currentTarget;
          follow = box.scrollHeight - box.scrollTop - box.clientHeight < FOLLOW_PX;
        }}
      >
        <Show when={canLoadOlder()}>
          <button type="button" class="chat-older" onClick={loadOlder}>
            Older messages
          </button>
        </Show>
        <Show when={message()}>
          <p class="chat-error" role="alert">
            {message()}
          </p>
        </Show>
        <Show when={items().length === 0}>
          <p class="chat-empty">Nothing is in the transcript yet.</p>
        </Show>
        {/* A group keeps its place when a new item comes, so its messages do not paint again. */}
        <Index each={groups()}>
          {(group) => (
            <Show
              when={group().kind === 'agent'}
              fallback={<For each={group().items}>{(item) => <Item item={item} from="you" />}</For>}
            >
              <Agent>
                <For each={group().items}>{(item) => <Item item={item} from="agent" />}</For>
              </Agent>
            </Show>
          )}
        </Index>
        <Show when={waiting().length > 0 || replyState(agent()) === 'working'}>
          <Agent>
            <For each={waiting()}>
              {(interaction) => <Card interaction={interaction} harness={harness()} />}
            </For>
            <Show when={replyState(agent()) === 'working'}>
              <p class="chat-working" role="status">
                {workingText(items())}
              </p>
            </Show>
          </Agent>
        </Show>
      </div>
      <Composer
        pane={props.pane}
        run={props.run}
        agent={agent()}
        harness={harness()}
        focused={props.focused}
      />
    </div>
  );
}

/** A small mark of the harness: the colour comes from `data-harness` of the chat. */
function Avatar(props: { harness: string | undefined }) {
  return (
    <svg class="chat-avatar" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <Show
        when={props.harness === 'codex'}
        fallback={<rect x="2" y="6" width="20" height="13" rx="6.5" fill="currentColor" />}
      >
        <circle cx="12" cy="12.5" r="9.5" fill="currentColor" />
      </Show>
      <circle cx="9" cy="12.5" r="1.5" class="chat-avatar-eye" />
      <circle cx="15" cy="12.5" r="1.5" class="chat-avatar-eye" />
    </svg>
  );
}
