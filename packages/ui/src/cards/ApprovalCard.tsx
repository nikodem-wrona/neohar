// The card of an approval: the command with its highlighted words, the
// directory, the risk and the reasons for it, the hint of the policy, the diff, the other input of
// the tool as text, and the last lines of the pane. The buttons are Allow, "Allow `<rule>` always"
// when the core sends a rule, Deny and Deny and stop. A high risk selects Deny, and Enter never
// answers it. The keys 1 to 4 name the buttons; a button keeps its key when "Allow always" is not
// there. At a low or medium risk a number answers. At a high risk it only selects its button and
// gives it the focus, so the answer takes a second key on the button.

import type { InteractionValue } from '@neohar/transport';
import { type KeyboardEvent, type Ref, useRef, useState } from 'react';
import { Input } from '../components/ui/input';
import { cn } from '../lib/utils';
import { Diff } from '../rich/Diff';
import { RichSpans } from '../rich/Spans';
import { sendAnswer } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';
import {
  type ApprovalChoice,
  approvalButtons,
  approvalDecision,
  buttonForKey,
  canAnswer,
  defaultChoice,
  enterAnswers,
} from './model';

type Approval = Extract<InteractionValue['payload'], { kind: 'approval' }>;

export function ApprovalCard(props: {
  interaction: InteractionValue;
  payload: Approval;
  harness: string;
  ref?: Ref<HTMLElement> | undefined;
}) {
  const host = useCardHost();
  const { report } = host;
  const [choice, setChoice] = useState<ApprovalChoice>(() => defaultChoice(props.interaction.risk));
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const command = props.payload.command;
  const shownButtons = approvalButtons(props.interaction);
  const buttons = useRef(new Map<ApprovalChoice, HTMLButtonElement>()).current;

  const answer = async (picked: ApprovalChoice) => {
    if (busy || !canAnswer(props.interaction)) return;
    setChoice(picked);
    setBusy(true);
    setError(await sendAnswer(host, props.interaction, approvalDecision(picked, message)));
    setBusy(false);
  };
  // Enter on the card answers with the selected choice, unless the risk is high. A button keeps
  // its own Enter.
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const typing = event.target instanceof HTMLInputElement;
    const numbered = buttonForKey(shownButtons, event.key);
    if (numbered && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      if (enterAnswers(props.interaction.risk)) {
        answer(numbered.id).catch(report);
      } else {
        setChoice(numbered.id);
        buttons.get(numbered.id)?.focus();
      }
      return;
    }
    if (event.key !== 'Enter' || event.target !== event.currentTarget) return;
    if (enterAnswers(props.interaction.risk)) answer(choice).catch(report);
  };

  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error}
      onKeyDown={onKeyDown}
      ref={props.ref}
    >
      {command ? (
        <>
          <pre className="card-command" aria-label="Command">
            <RichSpans value={command.display} />
          </pre>
          {command.reasons.length > 0 ? (
            <ul className="card-reasons" aria-label="Why this risk">
              {/* The reasons do not move: their place is their key. */}
              {command.reasons.map((reason, at) => (
                <li key={at}>{reason}</li>
              ))}
            </ul>
          ) : null}
          {command.policy_hint ? <p className="card-hint">{command.policy_hint}</p> : null}
        </>
      ) : null}
      {props.payload.cwd ? (
        <p className="card-cwd">
          <span className="card-label">In</span> {props.payload.cwd}
        </p>
      ) : null}
      {!command && (props.payload.paths?.length ?? 0) > 0 ? (
        <ul className="card-paths" aria-label="Files">
          {props.payload.paths?.map((path) => (
            <li key={path}>{path}</li>
          ))}
        </ul>
      ) : null}
      {props.payload.policy_note ? <p className="card-hint">{props.payload.policy_note}</p> : null}
      {props.payload.diff ? <Diff value={props.payload.diff} /> : null}
      {props.payload.input !== undefined && props.payload.input !== null ? (
        <details className="card-input">
          <summary>Input of {props.payload.tool}</summary>
          <pre>{JSON.stringify(props.payload.input, null, 2)}</pre>
        </details>
      ) : null}
      {(props.payload.peek?.length ?? 0) > 0 ? (
        <pre className="card-peek" role="region" aria-label="The last lines of the pane">
          {props.payload.peek?.join('\n')}
        </pre>
      ) : null}
      {canAnswer(props.interaction) ? (
        <>
          <div className="card-actions" role="group" aria-label="Answer">
            {shownButtons.map((button) => (
              <button
                key={button.id}
                type="button"
                className={cn('card-button', {
                  refuses: button.refuses,
                  selected: choice === button.id,
                })}
                data-choice={button.id}
                aria-pressed={choice === button.id}
                aria-keyshortcuts={String(button.key)}
                ref={(element) => {
                  if (element) buttons.set(button.id, element);
                  else buttons.delete(button.id);
                }}
                disabled={busy}
                onClick={() => void answer(button.id).catch(report)}
              >
                <kbd className="card-key" aria-hidden="true">
                  {button.key}
                </kbd>
                {button.label}
              </button>
            ))}
            <Input
              className="card-message"
              placeholder="Message for the agent (optional)"
              aria-label="Message for the agent"
              value={message}
              onChange={(event) => setMessage(event.currentTarget.value)}
            />
          </div>
          {!enterAnswers(props.interaction.risk) ? (
            <p className="card-note">
              Enter does not answer a high-risk request. Press a number, then Enter on its button.
            </p>
          ) : null}
        </>
      ) : null}
    </CardFrame>
  );
}
