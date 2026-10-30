// The Toaster of shadcn/ui (ui.shadcn.com, after Sonner), in Solid, on the tokens of the chrome:
// the toasts of the window (`app/toasts.ts`) in the bottom right corner, the newest at the
// bottom. Each one has the icon of its tone, a title, a description and a close button. The list
// is always in the page, so that a screen reader hears each new toast. While the pointer is on
// the list, no toast goes.

import { For, Match, Show, Switch } from 'solid-js';
import type { Toasts, ToastTone } from '../app/toasts';

export function Toaster(props: { toasts: Toasts }) {
  return (
    <section
      class="toaster"
      aria-label="Notifications"
      aria-live="polite"
      onPointerEnter={() => props.toasts.pause()}
      onPointerLeave={() => props.toasts.resume()}
    >
      <ol class="toaster-list">
        <For each={props.toasts.shown()}>
          {(toast) => (
            <li
              class="toast"
              data-tone={toast.tone}
              role={toast.tone === 'ok' ? 'status' : 'alert'}
            >
              <ToastIcon tone={toast.tone} />
              <p class="toast-title">{toast.title}</p>
              <Show when={toast.description}>
                <p class="toast-description">{toast.description}</p>
              </Show>
              <button
                type="button"
                class="toast-close"
                aria-label="Close the notification"
                onClick={() => props.toasts.dismiss(toast.id)}
              >
                <svg viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M4 4l8 8M12 4l-8 8" />
                </svg>
              </button>
            </li>
          )}
        </For>
      </ol>
    </section>
  );
}

/** A circle with a check, a triangle with a mark, or a circle with a cross. */
function ToastIcon(props: { tone: ToastTone }) {
  return (
    <svg class="toast-icon" viewBox="0 0 16 16" aria-hidden="true">
      <Switch>
        <Match when={props.tone === 'ok'}>
          <circle cx="8" cy="8" r="6.5" />
          <path d="M5.25 8.25l1.9 1.9 3.6-3.9" />
        </Match>
        <Match when={props.tone === 'warn'}>
          <path d="M8 2.25l6.25 11H1.75z" />
          <path d="M8 6.5v3M8 11.4v.1" />
        </Match>
        <Match when={props.tone === 'error'}>
          <circle cx="8" cy="8" r="6.5" />
          <path d="M5.75 5.75l4.5 4.5M10.25 5.75l-4.5 4.5" />
        </Match>
      </Switch>
    </svg>
  );
}
