// The banner for a link that is down. The host serves the projection `link` to each
// window. While its state is `connecting` or `disconnected` the window shows the text and the
// reason, and the surfaces take no input. The banner is driven by the projection only: the page
// guesses nothing about the network.

import { Show } from 'solid-js';
import { useApp } from '../app/context';

/** Whether the link of this window is down. A window without `link` is local. */
export function linkIsDown(state: string | undefined): boolean {
  return state === 'connecting' || state === 'disconnected';
}

export function LinkBanner() {
  const { stores } = useApp();
  const link = () => stores.link.state.byKey['_'];
  return (
    <Show when={linkIsDown(link()?.state) ? link() : undefined}>
      {(down) => (
        <div class="banner banner-link" role="alert" data-link-state={down().state}>
          <span class="banner-link-text">{down().text}</span>
          <Show when={down().reason}>
            <span class="banner-hint">{down().reason}</span>
          </Show>
        </div>
      )}
    </Show>
  );
}
