// The section Phone of the settings window: the state of the listener,
// the button that publishes it with Tailscale, the paired phones with a test of their push,
// and the dialog of a pairing.
// The core sends every text: the summary, the hint, the note of the offer and the QR code as an
// image. The section computes nothing: it shows the three projections and sends the methods.

import { type PhonePairingValue, RpcError, type Transport } from '@neohar/transport';
import { createEffect, createSignal, For, onCleanup, Show } from 'solid-js';
import { createProjectionStore } from '../stores';

function failureText(failure: unknown): string {
  return failure instanceof RpcError ? failure.message : 'The core did not answer.';
}

function PhoneRow(props: {
  transport: Transport;
  device: string;
  name: string;
  push: boolean;
  onError: (text: string | null) => void;
}) {
  const [editing, setEditing] = createSignal(false);
  const [sent, setSent] = createSignal(false);
  const done = (sent: Promise<unknown>): void => {
    sent
      .then(() => props.onError(null))
      .catch((failure: unknown) => props.onError(failureText(failure)));
  };
  return (
    <li class="phone-row" data-phone={props.device}>
      <Show when={editing()} fallback={<span class="phone-name">{props.name}</span>}>
        <input
          class="setting-input phone-name"
          aria-label="The name of the phone"
          value={props.name}
          ref={(input) => queueMicrotask(() => input.select())}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              const name = event.currentTarget.value.trim();
              setEditing(false);
              if (name !== '' && name !== props.name) {
                done(props.transport.call('phone.rename', { device: props.device, name }));
              }
            } else if (event.key === 'Escape') {
              setEditing(false);
            }
          }}
          onBlur={() => setEditing(false)}
        />
      </Show>
      <button type="button" class="setting-reset" onClick={() => setEditing(true)}>
        Rename
      </button>
      <button
        type="button"
        class="setting-reset"
        onClick={() => done(props.transport.call('phone.remove', { device: props.device }))}
      >
        Remove
      </button>
      <Show when={props.push}>
        <button
          type="button"
          class="setting-reset"
          onClick={() => {
            setSent(false);
            done(
              props.transport
                .call('phone.push_test', { device: props.device })
                .then(() => setSent(true)),
            );
          }}
        >
          Send a test notification
        </button>
        <Show when={sent()}>
          <span class="settings-note" role="status">
            Sent.
          </span>
        </Show>
      </Show>
    </li>
  );
}

/** The dialog of an offer: the QR code, the short code and the note of the core. */
function PairDialog(props: {
  pairing: PhonePairingValue | undefined;
  onAgain: () => void;
  onClose: () => void;
}) {
  const state = () => props.pairing?.state ?? 'none';
  return (
    <div
      class="dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        class="dialog phone-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="phone-dialog-title"
        tabIndex={-1}
        ref={(box) => queueMicrotask(() => box.focus())}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            props.onClose();
          }
        }}
      >
        <h2 id="phone-dialog-title" class="dialog-title">
          Pair a phone
        </h2>
        <Show
          when={state() === 'waiting' && props.pairing}
          fallback={
            <p class="dialog-text" role="status">
              {state() === 'expired' ? 'The offer ended.' : 'Waiting for the core.'}
            </p>
          }
        >
          {(pairing) => (
            <>
              <p class="dialog-text">
                Scan the code with the camera of the phone. On the phone, add the page to the home
                screen and open it from there.
              </p>
              <Show when={pairing().qr}>
                {(qr) => <img class="phone-qr" src={qr()} alt="The QR code of the pairing" />}
              </Show>
              <p class="phone-code-line">
                Or type this code in the app: <strong class="phone-code">{pairing().code}</strong>
              </p>
              <Show when={pairing().note}>{(note) => <p class="dialog-text">{note()}</p>}</Show>
              <p class="dialog-text" role="status">
                Waiting for the phone.
              </p>
            </>
          )}
        </Show>
        <div class="dialog-buttons">
          <Show when={state() === 'expired'}>
            <button type="button" class="primary" onClick={() => props.onAgain()}>
              New code
            </button>
          </Show>
          <button type="button" class="dialog-cancel" onClick={() => props.onClose()}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

export function PhoneSection(props: { transport: Transport }) {
  const status = createProjectionStore(props.transport, 'phone_status');
  const phones = createProjectionStore(props.transport, 'phones');
  const pairing = createProjectionStore(props.transport, 'phone_pairing');
  onCleanup(() => {
    status.close();
    phones.close();
    pairing.close();
  });
  const current = () => status.state.byKey['_'];
  const offer = () => pairing.state.byKey['_'];
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  // The dialog opens after `phone.pair_begin`. It closes when the projection, after it showed
  // the new offer, says that a phone paired.
  const [dialog, setDialog] = createSignal(false);
  const [seenOffer, setSeenOffer] = createSignal(false);

  createEffect(() => {
    if (!dialog()) return;
    const state = offer()?.state;
    if (state === 'waiting') setSeenOffer(true);
    else if (state === 'paired' && seenOffer()) setDialog(false);
  });

  const call = (method: 'phone.publish' | 'phone.unpublish'): void => {
    setBusy(true);
    props.transport
      .call(method, {})
      .then(() => setError(null))
      .catch((failure: unknown) => setError(failureText(failure)))
      .finally(() => setBusy(false));
  };

  const pair = (): void => {
    setSeenOffer(false);
    props.transport
      .call('phone.pair_begin', {})
      .then(() => {
        setError(null);
        setDialog(true);
      })
      .catch((failure: unknown) => setError(failureText(failure)));
  };

  const close = (): void => {
    setDialog(false);
    if (offer()?.state === 'waiting') {
      // The offer can end at the same time, and then there is nothing to cancel.
      props.transport.call('phone.pair_cancel', {}).catch(() => undefined);
    }
  };

  return (
    <div class="phone-section" data-phone-section>
      <p class="phone-summary" role="status">
        {current()?.summary ?? 'Waiting for the core.'}
      </p>
      <Show when={current()?.hint}>{(hint) => <p class="settings-note">{hint()}</p>}</Show>
      <div class="phone-actions">
        <Show when={current()?.tailscale}>
          {(tailscale) => (
            <Show
              when={tailscale().published}
              fallback={
                <button
                  type="button"
                  class="setting-reset"
                  disabled={busy()}
                  onClick={() => call('phone.publish')}
                >
                  Publish with Tailscale
                </button>
              }
            >
              <button
                type="button"
                class="setting-reset"
                disabled={busy()}
                onClick={() => call('phone.unpublish')}
              >
                Unpublish
              </button>
            </Show>
          )}
        </Show>
        <button type="button" class="setting-reset" onClick={pair}>
          Pair a phone
        </button>
      </div>
      <Show when={error()}>
        {(text) => (
          <p class="setting-error" role="alert">
            {text()}
          </p>
        )}
      </Show>
      <h2 class="settings-title phone-title">Paired phones</h2>
      <Show
        when={phones.rows().length > 0}
        fallback={<p class="settings-note">No phone is paired.</p>}
      >
        <ul class="phone-list" aria-label="Paired phones">
          <For each={phones.rows()}>
            {(phone) => (
              <PhoneRow
                transport={props.transport}
                device={phone.device}
                name={phone.name}
                push={phone.push === true}
                onError={setError}
              />
            )}
          </For>
        </ul>
      </Show>
      <h2 class="settings-title phone-title">Settings</h2>
      <Show when={dialog()}>
        <PairDialog pairing={offer()} onAgain={pair} onClose={close} />
      </Show>
    </div>
  );
}
