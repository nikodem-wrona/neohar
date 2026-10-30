// The banner for what is wrong with the core itself. A degraded store is a persistent
// banner: the commands that write are disabled, and the palette says why. The notes of the doctor
// follow, each with the hint that the core gives.

import { useApp } from '../app/context';
import { useSelect } from '../lib/hooks';

export function HealthBanner() {
  const { stores } = useApp();
  const health = useSelect(stores.health, (state) => state.byKey['_']);
  const storage = health?.storage;
  const degraded = storage?.state === 'degraded' ? storage : undefined;
  return (
    <>
      {degraded ? (
        <div className="banner banner-storage" role="alert">
          Storage is degraded: {degraded.reason}. Changes are not saved.
        </div>
      ) : null}
      {(health?.warnings ?? []).map((warning) => (
        <div
          key={warning.id}
          className="banner banner-warning"
          role="status"
          data-warning={warning.id}
        >
          <span>{warning.text}</span>
          {warning.hint ? <span className="banner-hint">{warning.hint}</span> : null}
        </div>
      ))}
    </>
  );
}
