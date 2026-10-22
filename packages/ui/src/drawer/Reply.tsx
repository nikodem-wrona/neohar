// The box Reply of a run: the next prompt for a run whose turn ended. On a phone, the
// line of such a run in the Inbox has it, so a tap on a push gives the field before the panes paint.
// `agent.reply` pastes the text into the pane and sends Enter. A send that fails keeps
// the text and its `input_id`, so a second try of the same text cannot write it twice. A new text
// gets a new id.

import { RpcError } from '@neohar/transport';
import { createSignal, Show } from 'solid-js';
import { useApp } from '../app/context';

export function Reply(props: { run: string; harness: string }) {
  const { transport, input, report } = useApp();
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let pending: number | null = null;
  const field = `reply-${props.run}`;

  const send = async (): Promise<void> => {
    const value = text();
    if (busy() || value.trim() === '') return;
    setBusy(true);
    setError(null);
    pending ??= input.nextId();
    try {
      await transport.call('agent.reply', { run: props.run, input_id: pending, text: value });
      pending = null;
      setText('');
    } catch (failure) {
      setError(failure instanceof RpcError ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      class="card-reply"
      aria-label="Reply"
      onSubmit={(event) => {
        event.preventDefault();
        send().catch(report);
      }}
    >
      <label class="card-label" for={field}>
        Reply to {props.harness}
      </label>
      <textarea
        id={field}
        class="card-field"
        rows="3"
        value={text()}
        onInput={(event) => {
          setText(event.currentTarget.value);
          pending = null;
        }}
      />
      <button type="submit" class="card-button" disabled={busy() || text().trim() === ''}>
        Send
      </button>
      <Show when={error()}>
        <p class="card-error" role="alert">
          {error()}
        </p>
      </Show>
    </form>
  );
}
