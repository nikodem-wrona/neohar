// The view Pair. The camera opens the address of the QR code in the browser, and
// the view tells the person to add the app to the home screen: the app of the home screen keeps its
// own cookies, so the pairing happens there. Its first start has the code in its address and pairs
// by itself. A person can also type the short code that the Mac shows.

import { type PhoneAccount, type PhoneDevice, RpcError } from '@neohar/transport';
import { useEffect, useState } from 'react';
import { useLatest } from '../lib/hooks';
import { pairCodeOf } from './model';

export function Pair(props: {
  account: PhoneAccount;
  search: string;
  standalone: boolean;
  onPaired: (device: PhoneDevice) => void;
}) {
  const code = pairCodeOf(props.search);
  const [short, setShort] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pair = (value: string): void => {
    if (busy || value.trim() === '') return;
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
  const start = useLatest(() => {
    if (props.standalone && code) pair(code);
  });
  useEffect(() => start.current(), [start]);

  return (
    <main className="phone-main phone-pair">
      <h1 className="phone-title">Pair this phone with the Mac</h1>
      {!props.standalone ? (
        <section className="phone-section" aria-label="Add neohar to the home screen">
          <ol className="phone-steps">
            <li>Tap the Share button of Safari.</li>
            <li>Tap “Add to Home Screen”.</li>
            <li>
              {code
                ? 'Open neohar from the home screen. It pairs by itself.'
                : 'Open neohar from the home screen, and type the code that the Mac shows.'}
            </li>
          </ol>
        </section>
      ) : null}
      <form
        className="phone-form"
        aria-label="Pair with a code"
        onSubmit={(event) => {
          event.preventDefault();
          pair(short);
        }}
      >
        <label className="phone-label" htmlFor="phone-code">
          The code that the Mac shows
        </label>
        <input
          id="phone-code"
          className="phone-field phone-code"
          type="text"
          inputMode="text"
          autoComplete="one-time-code"
          autoCapitalize="characters"
          spellCheck={false}
          maxLength={16}
          value={short}
          onChange={(event) => setShort(event.currentTarget.value)}
        />
        <button
          type="submit"
          className="phone-button selected"
          disabled={busy || short.trim() === ''}
        >
          Pair
        </button>
      </form>
      {busy ? (
        <p className="phone-empty" role="status">
          Pairing…
        </p>
      ) : null}
      {error ? (
        <p className="card-error" role="alert">
          {error}
        </p>
      ) : null}
    </main>
  );
}
