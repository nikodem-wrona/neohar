// The banner for what is wrong with the core itself. A degraded store is a persistent
// banner: the commands that write are disabled, and the palette says why. The notes of the doctor
// follow, each with the hint that the core gives.

import { For, Show } from 'solid-js';
import { useApp } from '../app/context';

export function HealthBanner() {
  const { stores } = useApp();
  const health = () => stores.health.state.byKey['_'];
  const storage = () => {
    const state = health()?.storage;
    return state?.state === 'degraded' ? state : undefined;
  };
  return (
    <>
      <Show when={storage()}>
        {(degraded) => (
          <div class="banner banner-storage" role="alert">
            Storage is degraded: {degraded().reason}. Changes are not saved.
          </div>
        )}
      </Show>
      <For each={health()?.warnings ?? []}>
        {(warning) => (
          <div class="banner banner-warning" role="status" data-warning={warning.id}>
            <span>{warning.text}</span>
            <Show when={warning.hint}>
              <span class="banner-hint">{warning.hint}</span>
            </Show>
          </div>
        )}
      </For>
    </>
  );
}
