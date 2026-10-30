// The inbox: the cards of the `interactions` projection, grouped by run, in the order that the
// core gives. A run that counts in `attention` without a card, because it is in `error`
// or `rate_limited` or its turn ended while nobody looked, has a line with its state
// and Open. The window keeps no state of its
// own about them. `⌘⇧A` asks the core which interaction is next and moves the focus to its card.
// The head of a group names the pane by its handle with `ui.show_handles`, else by its title.

import { useEffect, useRef } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { paneName, showHandles } from '../app/preferences';
import { showPane } from '../app/show';
import { Card } from '../cards/Card';
import { CARD, PART } from '../cards/CardFrame';
import { Button } from '../components/ui/button';
import {
  Card as CardBox,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '../components/ui/card';
import { cn } from '../lib/utils';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { groupByRun, replyState, type Trouble, troubleTitle } from './group';
import { Reply } from './Reply';

export function Inbox() {
  const { stores, view } = useApp();
  const cards = useRef(new Map<string, HTMLElement>()).current;
  // One ref for each card, so that a card that did not change does not render again.
  const refs = useRef(new Map<string, (element: HTMLElement | null) => void>()).current;
  const refOf = (interaction: string) => {
    let ref = refs.get(interaction);
    if (!ref) {
      ref = (element) => {
        if (element) cards.set(interaction, element);
        else {
          cards.delete(interaction);
          refs.delete(interaction);
        }
      };
      refs.set(interaction, ref);
    }
    return ref;
  };
  const interactions = useRows(stores.interactions);
  const attention = useRows(stores.attention);
  const agents = useSelect(stores.agents, (state) => state.byKey);
  const panes = useSelect(stores.panes, (state) => state.byKey);
  const settings = useSelect(stores.settings, (state) => state.byKey);
  const groups = groupByRun(interactions, attention);
  const harnessOf = (run: string | null) => {
    const agent = run ? agents[run] : undefined;
    return agent ? harnessName(agent.harness) : 'the agent';
  };
  const paneOf = (run: string | null) => {
    const agent = run ? agents[run] : undefined;
    const pane = agent?.pane ? panes[agent.pane] : undefined;
    return paneName(pane, showHandles(settings)) ?? null;
  };

  // The card takes the focus once it is in the page, and the request ends.
  const wanted = useValue(view.focusedCard);
  useEffect(() => {
    if (!wanted) return;
    const card = cards.get(wanted);
    if (!card) return;
    card.focus();
    card.scrollIntoView({ block: 'nearest' });
    view.focusCard(null);
  }, [wanted, groups, cards, view]);

  return (
    <div className="inbox">
      {groups.length > 0 ? (
        groups.map((group) => {
          const pane = paneOf(group.run);
          return (
            <section
              key={group.run ?? ''}
              className="inbox-group"
              aria-label={harnessOf(group.run)}
            >
              <h2 className="inbox-run">
                {harnessOf(group.run)}
                {pane ? <span className="inbox-pane"> · {pane}</span> : null}
              </h2>
              {group.trouble ? (
                <RunState run={group.run} trouble={group.trouble} harness={harnessOf(group.run)} />
              ) : null}
              {group.interactions.map((interaction) => (
                <Card
                  key={interaction.id}
                  interaction={interaction}
                  harness={harnessOf(group.run)}
                  ref={refOf(interaction.id)}
                />
              ))}
            </section>
          );
        })
      ) : (
        <p className="drawer-empty">Nothing waits for an answer.</p>
      )}
    </div>
  );
}

/**
 * The line of a run that counts in the inbox without a card: what is wrong, since when,
 * and Open, which shows the pane, in any tab, so the person sees what the harness shows.
 */
function RunState(props: { run: string | null; trouble: Trouble; harness: string }) {
  const { stores, transport, report, windowId, form } = useApp();
  const { run } = props;
  const agent = useSelect(stores.agents, (state) => (run ? state.byKey[run] : undefined));
  const since = useSelect(stores.attention, (state) => (run ? state.byKey[run]?.since : undefined));
  const title = troubleTitle(props.trouble);
  const text = (() => {
    switch (props.trouble) {
      case 'error':
        return `${props.harness} stopped with an error. Open the pane to see it.`;
      case 'rate_limited':
        return `${props.harness} waits for its limit to reset.`;
      case 'turn_ended':
        return `${props.harness} finished its turn. Open the pane to read it and reply.`;
    }
  })();
  const replyRun =
    form === 'phone' && props.trouble === 'turn_ended' && replyState(agent) === 'ready'
      ? run
      : null;
  const pane = agent?.pane;
  return (
    <CardBox asChild className={CARD}>
      <article data-state={props.trouble} tabIndex={0} aria-label={`${props.harness}: ${title}`}>
        <CardHeader className={PART}>
          {/* An error is red. A limit only waits: its title is dim. */}
          <CardTitle
            role="heading"
            aria-level={3}
            className={cn(
              'card-title leading-5',
              props.trouble === 'error' && 'text-destructive',
              props.trouble === 'rate_limited' && 'text-muted-foreground',
            )}
          >
            {title}
          </CardTitle>
        </CardHeader>
        <CardContent className={cn(PART, 'flex flex-col gap-(--s3)')}>
          <CardDescription className="text-[length:inherit] [overflow-wrap:anywhere]">
            {text}
          </CardDescription>
          {since ? (
            <p className="card-cwd">
              <span className="card-label">Since</span> <time dateTime={since}>{since}</time>
            </p>
          ) : null}
          {/* A phone answers a turn that ended in the Inbox. */}
          {replyRun ? <Reply run={replyRun} harness={props.harness} /> : null}
        </CardContent>
        {pane ? (
          <CardFooter className={PART} role="group" aria-label={title}>
            <Button
              variant="outline"
              size="sm"
              className="min-h-(--control)"
              onClick={() => void showPane(transport, windowId, pane).catch(report)}
            >
              Open
            </Button>
          </CardFooter>
        ) : null}
      </article>
    </CardBox>
  );
}
