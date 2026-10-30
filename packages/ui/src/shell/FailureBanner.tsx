// The banner for a command of this window that failed: the message of the core and its code. A
// new failure takes the place of the last one. The person closes it, or it goes after a time
// (`app/failures.ts`).

import { useApp } from '../app/context';
import { useValue } from '../lib/hooks';

export function FailureBanner() {
  const { failures } = useApp();
  const failure = useValue(failures.shown);
  // A new failure is a new banner.
  return failure ? (
    <div key={failure.id} className="banner banner-failure" role="alert" data-code={failure.code}>
      <span className="banner-failure-text">{failure.message}</span>
      <span className="banner-hint">{failure.code}</span>
      <button
        type="button"
        className="banner-close"
        aria-label="Close the message"
        onClick={() => failures.dismiss()}
      >
        ×
      </button>
    </div>
  ) : null;
}
