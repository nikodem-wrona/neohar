// The banner for a command of this window that failed: the message of the core and its code. A
// new failure takes the place of the last one. The person closes it, or it goes after a time
// (`app/failures.ts`).

import { useApp } from '../app/context';
import { Alert } from '../components/ui/alert';
import { Button } from '../components/ui/button';
import { useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { BANNER, BANNER_FAULT, BANNER_HINT } from './banner';

export function FailureBanner() {
  const { failures } = useApp();
  const failure = useValue(failures.shown);
  // A new failure is a new banner.
  return failure ? (
    <Alert
      key={failure.id}
      className={cn('banner-failure', BANNER, BANNER_FAULT)}
      role="alert"
      data-code={failure.code}
    >
      <span className="min-w-0 truncate">{failure.message}</span>
      <span className={BANNER_HINT}>{failure.code}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        className="ml-auto size-4 rounded-(--r-sm) text-muted-foreground hover:bg-(--hairline) hover:text-foreground dark:hover:bg-(--hairline)"
        aria-label="Close the message"
        onClick={() => failures.dismiss()}
      >
        ×
      </Button>
    </Alert>
  ) : null;
}
