// The Hermes view: chats with Hermes Agent, as a full view in the middle of the window, as the
// Git view is. The left column lists the open chats; the middle shows the items of one chat and a
// field for the next prompt.
//
// The core runs `hermes acp` and owns everything: `hermes_chats` has the chats with their state,
// and `hermes_chat:<chat>` the items of one chat as sanitized HTML, in their order. The view sends
// `hermes.*` calls and draws what comes back. Hermes keeps running a turn when the window or the
// app closes.

import type { HermesChatValue, HermesItem, HermesItemBody } from '@neohar/transport';
import { RpcError } from '@neohar/transport';
import { type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import { Badge } from '../components/ui/badge';
import { Button } from '../components/ui/button';
import { ScrollArea } from '../components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { Textarea } from '../components/ui/textarea';
import { windowFolder } from '../files/folder';
import { useRows } from '../lib/hooks';
import { cn } from '../lib/utils';
import { Diff } from '../rich/Diff';
import { RichHtml } from '../rich/RichHtml';
import { useProjectionStore } from '../stores';
import { answerLine, composer, folderName, stateOf, type Tone, toolMark } from './model';

const BADGE: Record<Tone, 'ok' | 'warn' | 'error' | 'muted'> = {
  live: 'ok',
  caution: 'warn',
  danger: 'error',
  dim: 'muted',
};

const TONE_TEXT: Record<Tone, string> = {
  live: 'text-live',
  caution: 'text-caution',
  danger: 'text-destructive',
  dim: 'text-muted-foreground',
};

function message(failure: unknown): string {
  return failure instanceof RpcError ? failure.message : String(failure);
}

export function HermesView() {
  const { stores, transport, windowId, view } = useApp();
  const chats = useRows(useProjectionStore(transport, 'hermes_chats'));
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const root = useRef<HTMLElement>(null);
  const shown = chats.find((chat) => chat.id === selected) ?? chats[0];

  useEffect(() => {
    root.current?.focus();
    transport
      .call('hermes.check', {})
      .then((status) => {
        if (!status.ok) setWarning(status.message ?? 'Hermes is not ready.');
        else if (status.approvals_mode && status.approvals_mode !== 'manual')
          setWarning(
            `Hermes has approvals.mode: ${status.approvals_mode}, so its shell commands do not ask here. File edits ask.`,
          );
      })
      .catch((failure) => setWarning(message(failure)));
  }, [transport]);

  const start = (): void => {
    const cwd = windowFolder(stores, windowId);
    if (!cwd) {
      setError('This window has no folder. Open a pane in a folder first.');
      return;
    }
    setError(null);
    transport
      .call('hermes.start', { cwd })
      .then((started) => setSelected(started.chat))
      .catch((failure) => setError(message(failure)));
  };

  return (
    <section
      className="hm-view"
      aria-label="Hermes view"
      tabIndex={-1}
      ref={root}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && event.target === event.currentTarget) view.closeHermes();
      }}
    >
      <header className="wf-bar">
        <span className="font-semibold">Hermes</span>
        <span className="wf-spacer" />
        <Button size="xs" onClick={start} title="A new chat in the folder of the window">
          New chat
        </Button>
      </header>
      {warning ? <p className="hm-banner">{warning}</p> : null}
      {error ? (
        <p className="hm-banner hm-danger" role="alert">
          {error}
        </p>
      ) : null}
      <div className="hm-body">
        <nav className="wf-side" aria-label="Chats">
          <ScrollArea className="min-h-0 flex-1">
            {chats.length === 0 ? (
              <p className="wf-empty wf-muted">No chat. New chat starts one in this folder.</p>
            ) : (
              <ul className="wf-list">
                {chats.map((chat) => (
                  <ChatRow
                    key={chat.id}
                    chat={chat}
                    selected={chat.id === shown?.id}
                    onSelect={() => setSelected(chat.id)}
                  />
                ))}
              </ul>
            )}
          </ScrollArea>
        </nav>
        {shown ? (
          <ChatPane key={shown.id} chat={shown} />
        ) : (
          <p className="git-empty git-center">Start a chat with Hermes Agent.</p>
        )}
      </div>
    </section>
  );
}

function ChatRow(props: { chat: HermesChatValue; selected: boolean; onSelect: () => void }) {
  const state = stateOf(props.chat);
  return (
    <li>
      <button
        type="button"
        className={cn('wf-list-item', props.selected && 'selected')}
        aria-current={props.selected ? 'true' : undefined}
        onClick={props.onSelect}
      >
        <span className="wf-list-name" title={props.chat.cwd}>
          {props.chat.title}
        </span>
        <Badge variant={BADGE[state.tone]}>{state.label}</Badge>
      </button>
    </li>
  );
}

function ChatPane(props: { chat: HermesChatValue }) {
  const { chat } = props;
  const { transport, report } = useApp();
  const items = useRows(useProjectionStore(transport, `hermes_chat:${chat.id}`));
  const end = useRef<HTMLDivElement>(null);
  const last = items.at(-1);

  // The newest item shows, as in a chat.
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' });
  }, [items.length, last]);

  const state = stateOf(chat);
  return (
    <div className="hm-chat">
      <header className="hm-chat-head">
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{chat.title}</div>
          <div className="wf-muted truncate text-xs" title={chat.cwd}>
            {folderName(chat.cwd)} · {chat.time_text}
            {chat.context_text ? ` · ${chat.context_text}` : ''}
          </div>
        </div>
        <Badge variant={BADGE[state.tone]}>{state.label}</Badge>
        <Choice
          label="Model"
          value={chat.model ?? undefined}
          choices={chat.models ?? []}
          onPick={(model) => {
            transport.call('hermes.set', { chat: chat.id, model }).catch(report);
          }}
        />
        <Choice
          label="Mode"
          value={chat.mode ?? undefined}
          choices={chat.modes ?? []}
          onPick={(mode) => {
            transport.call('hermes.set', { chat: chat.id, mode }).catch(report);
          }}
        />
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          title="Close the chat. The session stays in Hermes."
          onClick={() => {
            transport.call('hermes.close', { chat: chat.id }).catch(report);
          }}
        >
          Close
        </Button>
      </header>
      {chat.error ? (
        <div className="hm-banner hm-danger" role="alert">
          <span className="flex-1">{chat.error}</span>
          <Button
            size="xs"
            variant="outline"
            onClick={() => {
              transport.call('hermes.retry', { chat: chat.id }).catch(report);
            }}
          >
            Retry
          </Button>
        </div>
      ) : null}
      <ScrollArea className="min-h-0 flex-1">
        <ol className="hm-items" aria-label="Messages">
          {items.map((item) => (
            <Item key={item.id} chat={chat.id} item={item} />
          ))}
        </ol>
        <div ref={end} />
      </ScrollArea>
      <Composer chat={chat} />
    </div>
  );
}

function Choice(props: {
  label: string;
  value: string | undefined;
  choices: { id: string; name: string }[];
  onPick: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  useLayer(open);
  if (props.choices.length === 0) return null;
  return (
    <Select
      {...(props.value ? { value: props.value } : {})}
      open={open}
      onOpenChange={setOpen}
      onValueChange={props.onPick}
    >
      <SelectTrigger size="sm" className="h-7 max-w-[24ch]" aria-label={props.label}>
        <SelectValue placeholder={props.label} />
      </SelectTrigger>
      <SelectContent>
        {props.choices.map((choice) => (
          <SelectItem key={choice.id} value={choice.id}>
            {choice.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function Item(props: { chat: string; item: HermesItem }) {
  const body = props.item.body;
  return <li className={cn('hm-item', `hm-item-${body.type}`)}>{itemBody(props.chat, body)}</li>;
}

function itemBody(chat: string, body: HermesItemBody) {
  switch (body.type) {
    case 'prompt':
      return (
        <div className="hm-prompt-text">
          <RichHtml html={body.markdown.html} />
        </div>
      );
    case 'assistant':
      return (
        <div className={cn(body.streaming && 'hm-streaming')}>
          <RichHtml html={body.markdown.html} />
        </div>
      );
    case 'thought':
      return (
        <details className="hm-details">
          <summary className="wf-muted">Thinking</summary>
          <RichHtml html={body.markdown.html} />
        </details>
      );
    case 'tool': {
      const mark = toolMark(body.status);
      const line = (
        <>
          <span className={cn('hm-mark', TONE_TEXT[mark.tone])} aria-label={body.status}>
            {mark.mark}
          </span>
          <span className="truncate">{body.title}</span>
        </>
      );
      if (!body.output && !body.diff) return <div className="hm-tool-line">{line}</div>;
      return (
        <details className="hm-details">
          <summary className="hm-tool-line">{line}</summary>
          {body.diff ? <Diff value={body.diff} /> : null}
          {body.output ? <RichHtml html={body.output.html} /> : null}
        </details>
      );
    }
    case 'plan':
      return (
        <ul className="hm-plan" aria-label="Plan">
          {body.entries.map((entry, at) => (
            // The steps of a plan do not move: their place is their key.
            <li key={at} className={cn(entry.status === 'completed' && 'hm-plan-done')}>
              <span className="hm-mark" aria-hidden="true">
                {entry.status === 'completed' ? '☑' : entry.status === 'in_progress' ? '◐' : '☐'}
              </span>
              {entry.text}
            </li>
          ))}
        </ul>
      );
    case 'approval':
      return <Approval chat={chat} body={body} />;
    case 'notice':
      return <p className="wf-muted">{body.text}</p>;
  }
}

function Approval(props: { chat: string; body: Extract<HermesItemBody, { type: 'approval' }> }) {
  const { transport } = useApp();
  const { body } = props;
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const after = answerLine(body.state, body.answer ?? undefined);
  return (
    <div
      className={cn('hm-approval', body.state !== 'open' && 'hm-approval-closed')}
      role="group"
      aria-label="Approval"
    >
      <div className="font-semibold">{body.title}</div>
      {body.detail ? <RichHtml html={body.detail.html} /> : null}
      {body.diff ? <Diff value={body.diff} /> : null}
      {after ? (
        <p className="wf-muted">{after}</p>
      ) : (
        <div className="flex flex-wrap gap-(--s1)">
          {body.options.map((option) => (
            <Button
              key={option.id}
              size="sm"
              variant={option.allow ? 'default' : 'outline'}
              disabled={busy}
              onClick={() => {
                setBusy(true);
                setError(null);
                transport
                  .call('hermes.answer', {
                    chat: props.chat,
                    request: body.request,
                    option: option.id,
                  })
                  .catch((failure) => setError(message(failure)))
                  .finally(() => setBusy(false));
              }}
            >
              {option.label}
            </Button>
          ))}
        </div>
      )}
      {error ? (
        <p className="card-error" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function Composer(props: { chat: HermesChatValue }) {
  const { transport, report, input } = useApp();
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The id of the text that waits to go. A second try of the same text keeps it, so the core
  // sends it once.
  const pending = useRef<number | null>(null);
  const can = composer(props.chat);
  const field = `hermes-${props.chat.id}`;

  const send = async (): Promise<void> => {
    const value = text;
    if (busy || value.trim() === '') return;
    setBusy(true);
    setError(null);
    pending.current ??= input.nextId();
    try {
      await transport.call('hermes.prompt', {
        chat: props.chat.id,
        text: value,
        input_id: pending.current,
      });
      pending.current = null;
      setText('');
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  };

  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>): void => {
    event.stopPropagation();
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send().catch(report);
    } else if (event.key === 'Escape' && can.stop && text === '') {
      transport.call('hermes.cancel', { chat: props.chat.id }).catch(report);
    }
  };

  return (
    <form
      className="hm-composer"
      aria-label="Prompt"
      onSubmit={(event) => {
        event.preventDefault();
        send().catch(report);
      }}
    >
      <label className="sr-only" htmlFor={field}>
        Prompt for Hermes
      </label>
      <Textarea
        id={field}
        className="min-h-(--control) bg-background text-base md:text-sm"
        rows={3}
        placeholder="Ask Hermes. Enter sends, Shift+Enter makes a new line."
        value={text}
        onKeyDown={onKey}
        onChange={(event) => {
          setText(event.currentTarget.value);
          pending.current = null;
        }}
      />
      <div className="flex items-center gap-(--s1)">
        <span className="wf-muted flex-1 text-xs">{error ?? can.note ?? ''}</span>
        {can.stop ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => {
              transport.call('hermes.cancel', { chat: props.chat.id }).catch(report);
            }}
          >
            Stop
          </Button>
        ) : null}
        <Button type="submit" size="sm" disabled={busy || !can.send || text.trim() === ''}>
          Send
        </Button>
      </div>
    </form>
  );
}
