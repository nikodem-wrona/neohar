// Highlighted text as text nodes. Each run with a class is a `span`, and the rest is
// plain text. Nothing here builds HTML from a string.

import type { Spans } from '@neohar/transport';
import { For } from 'solid-js';
import { segments } from './segments';

export function RichSpans(props: { value: Spans }) {
  return (
    <For each={segments(props.value)}>
      {(run) => (run.className ? <span class={run.className}>{run.text}</span> : run.text)}
    </For>
  );
}
