// The banner for what is wrong with the core itself. A degraded store is a persistent
// banner: the commands that write are disabled, and the palette says why. The notes of the doctor
// follow, each with the hint that the core gives.

import { useApp } from '../app/context';
import { Alert } from '../components/ui/alert';
import { useSelect } from '../lib/hooks';
import { cn } from '../lib/utils';
import { BANNER, BANNER_HINT, BANNER_WARNING } from './banner';

export function HealthBanner() {
  const { stores } = useApp();
  const health = useSelect(stores.health, (state) => state.byKey['_']);
  const storage = health?.storage;
  const degraded = storage?.state === 'degraded' ? storage : undefined;
  return (
    <>
      {degraded ? (
        <Alert
          className={cn('banner-storage', BANNER, 'bg-destructive font-semibold text-on-signal')}
          role="alert"
        >
          Storage is degraded: {degraded.reason}. Changes are not saved.
        </Alert>
      ) : null}
      {(health?.warnings ?? []).map((warning) => (
        <Alert
          key={warning.id}
          className={cn('banner-warning', BANNER, BANNER_WARNING)}
          role="status"
          data-warning={warning.id}
        >
          <span>{warning.text}</span>
          {warning.hint ? <span className={BANNER_HINT}>{warning.hint}</span> : null}
        </Alert>
      ))}
    </>
  );
}
