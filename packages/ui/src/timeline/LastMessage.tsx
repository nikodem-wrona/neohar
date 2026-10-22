// The last message of the agent in full: an item of `timeline:<run>` has a preview of
// 300 characters only, so the Timeline tab asks `timeline.item` for the last message of the last
// turn and shows it as sanitized HTML. A new message, or a longer preview of it, asks again.

import { RpcError, type TimelineItem, type TimelineItemText } from '@neohar/transport';
import { createEffect, createSignal, on, Show } from 'solid-js';
import { useApp } from '../app/context';
import { RichHtml } from '../rich/RichHtml';
import { lineOf } from './model';

export function LastMessage(props: { run: string; item: TimelineItem }) {
  const { transport } = useApp();
  const [full, setFull] = createSignal<TimelineItemText | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  createEffect(
    on(
      () =>
        props.item.body.type === 'assistant'
          ? `${props.run}\n${props.item.id}\n${props.item.body.preview}`
          : null,
      (key) => {
        setFull(null);
        setError(null);
        if (key === null) return;
        const item = props.item.id;
        transport.call('timeline.item', { run: props.run, item }).then(
          (text) => {
            if (props.item.id === item) setFull(text);
          },
          (failure: unknown) =>
            setError(failure instanceof RpcError ? failure.message : String(failure)),
        );
      },
    ),
  );
  return (
    <section class="timeline-message-full" aria-label="Last message" data-item={props.item.id}>
      <Show
        when={full()}
        fallback={<p class="timeline-text">{error() ?? lineOf(props.item).text}</p>}
      >
        {(text) => (
          <>
            <RichHtml html={text().html} />
            <Show when={text().truncated}>
              <p class="timeline-cut">The message is cut at the size limit.</p>
            </Show>
          </>
        )}
      </Show>
    </section>
  );
}
