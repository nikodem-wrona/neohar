// The banner for a command of this window that failed: the message of the core and its code. A
// new failure takes the place of the last one. The person closes it, or it goes after a time
// (`app/failures.ts`).

import { Show } from 'solid-js';
import { useApp } from '../app/context';

export function FailureBanner() {
  const { failures } = useApp();
  return (
    <Show when={failures.shown()} keyed>
      {(failure) => (
        <div class="banner banner-failure" role="alert" data-code={failure.code}>
          <span class="banner-failure-text">{failure.message}</span>
          <span class="banner-hint">{failure.code}</span>
          <button
            type="button"
            class="banner-close"
            aria-label="Close the message"
            onClick={() => failures.dismiss()}
          >
            ×
          </button>
        </div>
      )}
    </Show>
  );
}
