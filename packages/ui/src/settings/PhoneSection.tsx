// The section Phone of the settings window: the state of the listener,
// the button that publishes it with Tailscale, the paired phones with a test of their push,
// and the dialog of a pairing.
// The core sends every text: the summary, the hint, the note of the offer and the QR code as an
// image. The section computes nothing: it shows the three projections and sends the methods.

import { type PhonePairingValue, RpcError, type Transport } from '@neohar/transport';
import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogTitle } from '../components/ui/dialog';
import { useRows, useValue } from '../lib/hooks';
import { useProjectionStore } from '../stores';

function failureText(failure: unknown): string {
  return failure instanceof RpcError ? failure.message : 'The core did not answer.';
}

/** Selects the text of the field when it shows. */
function selectWhenShown(input: HTMLInputElement | null): void {
  if (input) queueMicrotask(() => input.select());
}

function PhoneRow(props: {
  transport: Transport;
  device: string;
  name: string;
  push: boolean;
  onError: (text: string | null) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [sent, setSent] = useState(false);
  const done = (sent: Promise<unknown>): void => {
    sent
      .then(() => props.onError(null))
      .catch((failure: unknown) => props.onError(failureText(failure)));
  };
  return (
    <li className="phone-row" data-phone={props.device}>
      {editing ? (
        <input
          className="setting-input phone-name"
          aria-label="The name of the phone"
          defaultValue={props.name}
          ref={selectWhenShown}
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
      ) : (
        <span className="phone-name">{props.name}</span>
      )}
      <button type="button" className="setting-reset" onClick={() => setEditing(true)}>
        Rename
      </button>
      <button
        type="button"
        className="setting-reset"
        onClick={() => done(props.transport.call('phone.remove', { device: props.device }))}
      >
        Remove
      </button>
      {props.push ? (
        <>
          <button
            type="button"
            className="setting-reset"
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
          {sent ? (
            <span className="settings-note" role="status">
              Sent.
            </span>
          ) : null}
        </>
      ) : null}
    </li>
  );
}

/** The dialog of an offer: the QR code, the short code and the note of the core. */
function PairDialog(props: {
  pairing: PhonePairingValue | undefined;
  onAgain: () => void;
  onClose: () => void;
}) {
  const state = props.pairing?.state ?? 'none';
  const pairing = state === 'waiting' ? props.pairing : undefined;
  // The dialog opens when the core answers, with no trigger: it gives the focus back to the
  // element that had it. The settings window has no panes, so the dialog needs no layer.
  const [before] = useState(() => document.activeElement);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
    >
      <DialogContent
        className="max-h-[calc(92vh-16px)] w-[min(48ch,calc(100vw-32px))] max-w-none gap-2 overflow-auto px-3 pt-0 pb-3 sm:max-w-none"
        showCloseButton={false}
        aria-describedby={undefined}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (before instanceof HTMLElement) before.focus();
        }}
      >
        <DialogTitle className="dialog-title text-[length:inherit] leading-[inherit]">
          Pair a phone
        </DialogTitle>
        {pairing ? (
          <>
            <p className="dialog-text">
              Scan the code with the camera of the phone. On the phone, add the page to the home
              screen and open it from there.
            </p>
            {pairing.qr ? (
              <img className="phone-qr" src={pairing.qr} alt="The QR code of the pairing" />
            ) : null}
            <p className="phone-code-line">
              Or type this code in the app: <strong className="phone-code">{pairing.code}</strong>
            </p>
            {pairing.note ? <p className="dialog-text">{pairing.note}</p> : null}
            <p className="dialog-text" role="status">
              Waiting for the phone.
            </p>
          </>
        ) : (
          <p className="dialog-text" role="status">
            {state === 'expired' ? 'The offer ended.' : 'Waiting for the core.'}
          </p>
        )}
        <div className="dialog-buttons">
          {state === 'expired' ? (
            <button type="button" className="primary" onClick={() => props.onAgain()}>
              New code
            </button>
          ) : null}
          <button type="button" className="dialog-cancel" onClick={() => props.onClose()}>
            Cancel
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function PhoneSection(props: { transport: Transport }) {
  const { transport } = props;
  const status = useProjectionStore(transport, 'phone_status');
  const phones = useProjectionStore(transport, 'phones');
  const pairing = useProjectionStore(transport, 'phone_pairing');
  const current = useValue(status).byKey['_'];
  const offer = useValue(pairing).byKey['_'];
  const phoneRows = useRows(phones);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // The dialog opens after `phone.pair_begin`. It closes when the projection, after it showed
  // the new offer, says that a phone paired.
  const [dialog, setDialog] = useState(false);
  const [seenOffer, setSeenOffer] = useState(false);

  const offerState = offer?.state;
  useEffect(() => {
    if (!dialog) return;
    if (offerState === 'waiting') setSeenOffer(true);
    else if (offerState === 'paired' && seenOffer) setDialog(false);
  }, [dialog, offerState, seenOffer]);

  const call = (method: 'phone.publish' | 'phone.unpublish'): void => {
    setBusy(true);
    transport
      .call(method, {})
      .then(() => setError(null))
      .catch((failure: unknown) => setError(failureText(failure)))
      .finally(() => setBusy(false));
  };

  const pair = (): void => {
    setSeenOffer(false);
    transport
      .call('phone.pair_begin', {})
      .then(() => {
        setError(null);
        setDialog(true);
      })
      .catch((failure: unknown) => setError(failureText(failure)));
  };

  const close = (): void => {
    setDialog(false);
    if (offer?.state === 'waiting') {
      // The offer can end at the same time, and then there is nothing to cancel.
      transport.call('phone.pair_cancel', {}).catch(() => undefined);
    }
  };

  return (
    <div className="phone-section" data-phone-section>
      <p className="phone-summary" role="status">
        {current?.summary ?? 'Waiting for the core.'}
      </p>
      {current?.hint ? <p className="settings-note">{current.hint}</p> : null}
      <div className="phone-actions">
        {current?.tailscale ? (
          current.tailscale.published ? (
            <button
              type="button"
              className="setting-reset"
              disabled={busy}
              onClick={() => call('phone.unpublish')}
            >
              Unpublish
            </button>
          ) : (
            <button
              type="button"
              className="setting-reset"
              disabled={busy}
              onClick={() => call('phone.publish')}
            >
              Publish with Tailscale
            </button>
          )
        ) : null}
        <button type="button" className="setting-reset" onClick={pair}>
          Pair a phone
        </button>
      </div>
      {error ? (
        <p className="setting-error" role="alert">
          {error}
        </p>
      ) : null}
      <h2 className="settings-title phone-title">Paired phones</h2>
      {phoneRows.length > 0 ? (
        <ul className="phone-list" aria-label="Paired phones">
          {phoneRows.map((phone) => (
            <PhoneRow
              key={phone.device}
              transport={transport}
              device={phone.device}
              name={phone.name}
              push={phone.push === true}
              onError={setError}
            />
          ))}
        </ul>
      ) : (
        <p className="settings-note">No phone is paired.</p>
      )}
      <h2 className="settings-title phone-title">Settings</h2>
      {dialog ? <PairDialog pairing={offer} onAgain={pair} onClose={close} /> : null}
    </div>
  );
}
