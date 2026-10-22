// The view Pair. The camera opens the address of the QR code in the browser, and
// the view tells the person to add the app to the home screen: the app of the home screen keeps its
// own cookies, so the pairing happens there. Its first start has the code in its address and pairs
// by itself. A person can also type the short code that the Mac shows.

import { type PhoneAccount, type PhoneDevice, RpcError } from '@neohar/transport';
import { createSignal, onMount, Show } from 'solid-js';
import { pairCodeOf } from './model';

export function Pair(props: {
  account: PhoneAccount;
  search: string;
  standalone: boolean;
  onPaired: (device: PhoneDevice) => void;
}) {
  const code = pairCodeOf(props.search);
  const [short, setShort] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const pair = (value: string): void => {
    if (busy() || value.trim() === '') return;
    setBusy(true);
    setError(null);
    props.account.pair(value).then(
      (device) => {
        // The code is spent. The address keeps no copy of it.
        history.replaceState(null, '', `/${location.hash}`);
        props.onPaired(device);
      },
      (failure: unknown) => {
        setBusy(false);
        setError(failure instanceof RpcError ? failure.message : String(failure));
      },
    );
  };
  // The first start of the app of the home screen has the code of the QR code.
  onMount(() => {
    if (props.standalone && code) pair(code);
  });

  return (
    <main class="phone-main phone-pair">
      <h1 class="phone-title">Pair this phone with the Mac</h1>
      <Show when={!props.standalone}>
        <section class="phone-section" aria-label="Add neohar to the home screen">
          <ol class="phone-steps">
            <li>Tap the Share button of Safari.</li>
            <li>Tap “Add to Home Screen”.</li>
            <li>
              {code
                ? 'Open neohar from the home screen. It pairs by itself.'
                : 'Open neohar from the home screen, and type the code that the Mac shows.'}
            </li>
          </ol>
        </section>
      </Show>
      <form
        class="phone-form"
        aria-label="Pair with a code"
        onSubmit={(event) => {
          event.preventDefault();
          pair(short());
        }}
      >
        <label class="phone-label" for="phone-code">
          The code that the Mac shows
        </label>
        <input
          id="phone-code"
          class="phone-field phone-code"
          type="text"
          inputmode="text"
          autocomplete="one-time-code"
          autocapitalize="characters"
          spellcheck={false}
          maxlength="16"
          value={short()}
          onInput={(event) => setShort(event.currentTarget.value)}
        />
        <button
          type="submit"
          class="phone-button selected"
          disabled={busy() || short().trim() === ''}
        >
          Pair
        </button>
      </form>
      <Show when={busy()}>
        <p class="phone-empty" role="status">
          Pairing…
        </p>
      </Show>
      <Show when={error()}>
        <p class="card-error" role="alert">
          {error()}
        </p>
      </Show>
    </main>
  );
}
