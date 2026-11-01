// The banner for a link that is down. The host serves the projection `link` to each
// window. While its state is `connecting` or `disconnected` the window shows the text and the
// reason, and the surfaces take no input. The banner is driven by the projection only: the page
// guesses nothing about the network.

import { useApp } from '../app/context';
import { Alert } from '../components/ui/alert';
import { useSelect } from '../lib/hooks';
import { cn } from '../lib/utils';
import { BANNER, BANNER_FAULT, BANNER_HINT } from './banner';

/** Whether the link of this window is down. A window without `link` is local. */
export function linkIsDown(state: string | undefined): boolean {
  return state === 'connecting' || state === 'disconnected';
}

export function LinkBanner() {
  const { stores } = useApp();
  const link = useSelect(stores.link, (state) => state.byKey['_']);
  const down = linkIsDown(link?.state) ? link : undefined;
  return down ? (
    <Alert
      className={cn('banner-link', BANNER, BANNER_FAULT)}
      role="alert"
      data-link-state={down.state}
    >
      <span className="font-semibold">{down.text}</span>
      {down.reason ? <span className={BANNER_HINT}>{down.reason}</span> : null}
    </Alert>
  ) : null;
}
