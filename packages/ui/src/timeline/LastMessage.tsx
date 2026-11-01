// The last message of the agent in full: an item of `timeline:<run>` has a preview of
// 300 characters only, so the Timeline tab asks `timeline.item` for the last message of the last
// turn and shows it as sanitized HTML. A new message, or a longer preview of it, asks again.

import { RpcError, type TimelineItem, type TimelineItemText } from '@neohar/transport';
import { useEffect, useState } from 'react';
import { useApp } from '../app/context';
import { useLatest } from '../lib/hooks';
import { RichHtml } from '../rich/RichHtml';
import { lineOf } from './model';

export function LastMessage(props: { run: string; item: TimelineItem }) {
  const { transport } = useApp();
  const [full, setFull] = useState<TimelineItemText | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { run } = props;
  const item = props.item.id;
  const shownItem = useLatest(item);
  const key =
    props.item.body.type === 'assistant' ? `${run}\n${item}\n${props.item.body.preview}` : null;
  useEffect(() => {
    setFull(null);
    setError(null);
    if (key === null) return;
    transport.call('timeline.item', { run, item }).then(
      (text) => {
        if (shownItem.current === item) setFull(text);
      },
      (failure: unknown) =>
        setError(failure instanceof RpcError ? failure.message : String(failure)),
    );
  }, [key, run, item, transport, shownItem]);
  return (
    <section className="timeline-message-full" aria-label="Last message" data-item={props.item.id}>
      {full ? (
        <>
          <RichHtml html={full.html} />
          {full.truncated ? (
            <p className="timeline-cut">The message is cut at the size limit.</p>
          ) : null}
        </>
      ) : (
        <p className="timeline-text">{error ?? lineOf(props.item).text}</p>
      )}
    </section>
  );
}
