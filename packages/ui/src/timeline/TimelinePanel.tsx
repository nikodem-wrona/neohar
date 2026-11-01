// The Timeline tab of the drawer: the turns of the agent in the pane that has the focus.
// The core builds the items from the transcript of the run (`timeline:<run>`) and gives
// older ones through `timeline.page`. The last message of the agent shows in full under the turns.
// The window follows the projection only while this tab shows,
// so the core reads the file for no one else.

import { RpcError, type TimelineItem } from '@neohar/transport';
import { useState } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { useRows, useSelect } from '../lib/hooks';
import { useProjectionStore } from '../stores';
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
  const { stores, windowId } = useApp();
  const focused = useSelect(stores.windows, (state) => state.byKey[windowId]?.focused_pane);
  const pane = useSelect(stores.panes, (state) => (focused ? state.byKey[focused] : undefined));
  const run = focusedRun(pane);

  return (
    <div className="timeline-panel">
      {/* A new run is a new timeline: its own subscription, older items and message. */}
      {run ? (
        <RunTimeline key={run} run={run} />
      ) : (
        <p className="drawer-empty">No agent is in focus.</p>
      )}
    </div>
  );
}

function RunTimeline(props: { run: string }) {
  const { stores, transport } = useApp();
  const { run } = props;
  const agent = useSelect(stores.agents, (state) => state.byKey[run]);
  const [older, setOlder] = useState<TimelineItem[]>([]);
  const [more, setMore] = useState<boolean | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  // One subscription for the run in focus. It ends when the focus moves or the tab closes.
  const live = useProjectionStore(transport, `timeline:${run}`);
  const liveRows = useRows(live);
  const items = joinPages(older, liveRows);
  const turns = groupByTurn(items);
  const last = lastMessage(turns[turns.length - 1]);
  // With fewer items than the projection can hold, no older item exists.
  const canLoadOlder = more ?? liveRows.length >= LIVE_ITEMS;

  const loadOlder = (): void => {
    const first = items[0];
    if (!first) return;
    setMessage(null);
    transport
      .call('timeline.page', { run, before_item: first.id, limit: PAGE })
      .then((result) => {
        setOlder((previous) => [...result.items, ...previous]);
        setMore(result.more);
      })
      .catch((failure: unknown) => {
        setMessage(failure instanceof RpcError ? failure.message : 'The page did not load.');
      });
  };

  return (
    <>
      <header className="timeline-head">
        <span className="timeline-agent">{agent ? harnessName(agent.harness) : 'Agent'}</span>
        {agent && hasUsage(agent.usage) ? (
          <span className="timeline-usage" data-testid="timeline-usage">
            {usageText(agent.usage)}
          </span>
        ) : null}
        {agent?.rate_limit ? (
          <span className="timeline-limit">{windowText(agent.rate_limit)}</span>
        ) : null}
      </header>
      {message ? (
        <p className="timeline-message" role="alert">
          {message}
        </p>
      ) : null}
      {items.length > 0 ? (
        <>
          {canLoadOlder ? (
            <button type="button" className="timeline-older" onClick={loadOlder}>
              Older items
            </button>
          ) : null}
          {turns.map((turn) => (
            <section key={turn.turn} className="timeline-turn" aria-label={`Turn ${turn.turn}`}>
              <h3 className="timeline-turn-title">Turn {turn.turn}</h3>
              <ul className="timeline-items">
                {turn.items.map((item) => {
                  const line = lineOf(item);
                  return (
                    <li
                      key={item.id}
                      className={`timeline-item timeline-${line.kind}`}
                      data-item={item.id}
                    >
                      <span className="timeline-label">{line.label}</span>
                      {line.status ? (
                        <span className="timeline-status" data-status={line.status}>
                          {line.status}
                        </span>
                      ) : null}
                      {line.kind === 'output' ? (
                        <pre className="timeline-text">{line.text}</pre>
                      ) : (
                        <span className="timeline-text">{line.text}</span>
                      )}
                      {line.truncated ? <span className="timeline-cut">cut</span> : null}
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
          {last !== undefined ? <LastMessage run={run} item={last} /> : null}
        </>
      ) : (
        <p className="drawer-empty">Nothing is in the transcript yet.</p>
      )}
    </>
  );
}
