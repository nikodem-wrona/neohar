// The banner for a link that is down. The host serves the projection `link` to each
// window. While its state is `connecting` or `disconnected` the window shows the text and the
// reason, and the surfaces take no input. The banner is driven by the projection only: the page
// guesses nothing about the network.

import { useApp } from '../app/context';
import { useSelect } from '../lib/hooks';

/** Whether the link of this window is down. A window without `link` is local. */
export function linkIsDown(state: string | undefined): boolean {
  return state === 'connecting' || state === 'disconnected';
}

export function LinkBanner() {
  const { stores } = useApp();
  const link = useSelect(stores.link, (state) => state.byKey['_']);
  const down = linkIsDown(link?.state) ? link : undefined;
  return down ? (
    <div className="banner banner-link" role="alert" data-link-state={down.state}>
      <span className="banner-link-text">{down.text}</span>
      {down.reason ? <span className="banner-hint">{down.reason}</span> : null}
    </div>
  ) : null;
}
