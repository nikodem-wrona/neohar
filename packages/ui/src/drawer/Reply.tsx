// The box Reply of a run: the next prompt for a run whose turn ended. On a phone, the
// line of such a run in the Inbox has it, so a tap on a push gives the field before the panes paint.
// `agent.reply` pastes the text into the pane and sends Enter. A send that fails keeps
// the text and its `input_id`, so a second try of the same text cannot write it twice. A new text
// gets a new id.

import { RpcError } from '@neohar/transport';
import { useRef, useState } from 'react';
import { useApp } from '../app/context';
import { Button } from '../components/ui/button';
import { Textarea } from '../components/ui/textarea';

export function Reply(props: { run: string; harness: string }) {
  const { transport, input, report } = useApp();
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef<number | null>(null);
  const field = `reply-${props.run}`;

  const send = async (): Promise<void> => {
    const value = text;
    if (busy || value.trim() === '') return;
    setBusy(true);
    setError(null);
    pending.current ??= input.nextId();
    try {
      await transport.call('agent.reply', {
        run: props.run,
        input_id: pending.current,
        text: value,
      });
      pending.current = null;
      setText('');
    } catch (failure) {
      setError(failure instanceof RpcError ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="flex flex-col gap-(--s1)"
      aria-label="Reply"
      onSubmit={(event) => {
        event.preventDefault();
        send().catch(report);
      }}
    >
      <label className="card-label" htmlFor={field}>
        Reply to {props.harness}
      </label>
      {/* A font of 16 px, so that Safari does not zoom into the field. */}
      <Textarea
        id={field}
        className="min-h-(--control) bg-background font-mono text-base md:text-base"
        rows={3}
        value={text}
        onChange={(event) => {
          setText(event.currentTarget.value);
          pending.current = null;
        }}
      />
      <Button type="submit" className="min-h-(--control)" disabled={busy || text.trim() === ''}>
        Send
      </Button>
      {error ? (
        <p className="card-error" role="alert">
          {error}
        </p>
      ) : null}
    </form>
  );
}
