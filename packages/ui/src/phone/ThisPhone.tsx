// The view This phone: the name of the phone, a field to change it, the switch
// Notifications with "Send a test notification", and "Unpair this phone", which removes
// the phone from the Mac. The switch shows what the row of the phone in `phones` says: the core
// knows whether the phone has a push subscription.

import { RpcError } from '@neohar/transport';
import { createSignal, Show } from 'solid-js';
import { usePhone } from './context';
import { turnOff, turnOn } from './push';

const message = (failure: unknown) =>
  failure instanceof RpcError || failure instanceof Error ? failure.message : String(failure);

export function ThisPhone(props: { restart: () => void }) {
  const { stores, transport, account, device, push } = usePhone();
  const name = () => stores.phones.state.byKey[device.device]?.name ?? device.name;
  const pushOn = () => stores.phones.state.byKey[device.device]?.push === true;
  const [draft, setDraft] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [note, setNote] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [pushBusy, setPushBusy] = createSignal(false);

  // The switch shows the row of the core. A change that fails puts the switch back.
  const toggle = (input: HTMLInputElement): void => {
    setPushBusy(true);
    setError(null);
    setNote(null);
    const vapidKey = stores.status.state.byKey['_']?.vapid_key;
    const sent = input.checked ? turnOn(push, transport, vapidKey) : turnOff(push, transport);
    sent
      .catch((failure: unknown) => {
        setError(message(failure));
        input.checked = pushOn();
      })
      .finally(() => setPushBusy(false));
  };
  const test = (): void => {
    setPushBusy(true);
    setError(null);
    setNote(null);
    transport
      .call('phone.push_test', { device: device.device })
      .then(() => setNote('The Mac sent a test notification.'))
      .catch((failure: unknown) => setError(message(failure)))
      .finally(() => setPushBusy(false));
  };

  const rename = (): void => {
    const value = draft();
    if (value === null || value.trim() === '' || value === name()) return;
    setError(null);
    transport.call('phone.rename', { device: device.device, name: value }).then(
      () => setDraft(null),
      (failure: unknown) => setError(message(failure)),
    );
  };
  const unpair = (): void => {
    setBusy(true);
    setError(null);
    account.unpair().then(props.restart, (failure: unknown) => {
      setBusy(false);
      setError(message(failure));
    });
  };

  return (
    <section class="phone-section" aria-labelledby="phone-this">
      <h1 class="phone-title" id="phone-this">
        This phone
      </h1>
      <form
        class="phone-form"
        aria-label="Name of this phone"
        onSubmit={(event) => {
          event.preventDefault();
          rename();
        }}
      >
        <label class="phone-label" for="phone-name">
          Name
        </label>
        <input
          id="phone-name"
          class="phone-field"
          type="text"
          maxlength="64"
          value={draft() ?? name()}
          onInput={(event) => setDraft(event.currentTarget.value)}
        />
        <button
          type="submit"
          class="phone-button"
          disabled={draft() === null || draft()?.trim() === '' || draft() === name()}
        >
          Rename
        </button>
      </form>
      <label class="phone-switch">
        <input
          type="checkbox"
          role="switch"
          checked={pushOn()}
          disabled={pushBusy()}
          onChange={(event) => toggle(event.currentTarget)}
        />
        <span>Notifications</span>
      </label>
      <p class="phone-note">
        The Mac sends a notification when an agent needs you and no window of neohar has the focus.
        A notification names the agent and the reason, never the text of the agent.
      </p>
      <Show when={pushOn()}>
        <button type="button" class="phone-button" disabled={pushBusy()} onClick={test}>
          Send a test notification
        </button>
      </Show>
      <Show when={note()}>
        <p class="phone-note" role="status">
          {note()}
        </p>
      </Show>
      <p class="phone-note">
        A phone that the Mac knows can answer the agents and send them prompts. Unpair a phone that
        you do not use.
      </p>
      <button type="button" class="phone-button danger" disabled={busy()} onClick={unpair}>
        Unpair this phone
      </button>
      <Show when={error()}>
        <p class="card-error" role="alert">
          {error()}
        </p>
      </Show>
    </section>
  );
}
