// Sanitized HTML from the core: the plan of an agent as Markdown. The core cleans it with
// `ammonia`, so scripts, event handlers and styles are gone before the UI sees it. The HTML of a
// remote machine is cleaned again by the link client of this Mac, with the same rules, because a
// hostile core can send any text. Insert only a field `html` of the API here: the client cleans
// those fields and no others.
// This component is the only user of `innerHTML` in the UI. `contain: content` (in the CSS) keeps
// the layout of the page out of its reach. A click on a link never navigates the page: in the
// app, the click asks the host to open the link outside the app, with the gesture token of the
// host. On a phone, a link with `https` opens in Safari, and a link to an address of the loopback
// interface says that the address is on the Mac.

import { createEffect, createSignal, Show } from 'solid-js';
import { useAppMaybe } from '../app/context';
import { writeClipboard } from '../input/clipboard';
import { clickedLink, gestureToken, isLoopback } from './links';

export function RichHtml(props: { html: string }) {
  let root: HTMLDivElement | undefined;
  const app = useAppMaybe();
  const phone = app?.form === 'phone';
  const [onMac, setOnMac] = createSignal<string | null>(null);
  createEffect(() => {
    const html = props.html;
    if (root) root.innerHTML = html;
  });
  const keepLinks = (event: MouseEvent) => {
    const target = event.target;
    const link = target instanceof Element ? target.closest('a') : null;
    if (!link) return;
    event.preventDefault();
    const href = link.getAttribute('href') ?? '';
    if (!phone) {
      // The host cancels this navigation and opens the address of the link outside the app.
      const open = clickedLink(href, gestureToken());
      if (open) window.location.assign(open);
      return;
    }
    if (isLoopback(href)) setOnMac(href);
    else if (href.startsWith('https://')) window.open(href, '_blank', 'noopener,noreferrer');
  };
  return (
    <>
      <div class="rich-html" ref={(element) => (root = element)} onClick={keepLinks} />
      <Show when={onMac()}>
        {(href) => (
          <p class="rich-note" role="status">
            This address is on the Mac: <code>{href()}</code>
            <button
              type="button"
              class="card-button"
              onClick={() => {
                void writeClipboard(Promise.resolve(href())).catch((failure: unknown) =>
                  app?.report(failure),
                );
                setOnMac(null);
              }}
            >
              Copy
            </button>
          </p>
        )}
      </Show>
    </>
  );
}
