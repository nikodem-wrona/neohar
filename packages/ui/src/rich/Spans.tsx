// Highlighted text as text nodes. Each run with a class is a `span`, and the rest is
// plain text. Nothing here builds HTML from a string.

import type { Spans } from '@neohar/transport';
import { segments } from './segments';

export function RichSpans(props: { value: Spans }) {
  // The runs of a text do not move: their place is their key.
  return (
    <>
      {segments(props.value).map((run, at) =>
        run.className ? (
          <span key={at} className={run.className}>
            {run.text}
          </span>
        ) : (
          run.text
        ),
      )}
    </>
  );
}
