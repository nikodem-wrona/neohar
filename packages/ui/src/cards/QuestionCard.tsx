// The card of a question: the last message of the agent, each question with its options as
// choice cards, a field for an answer of the person's own, and one button that sends all the
// answers. The header of one question is the title of the card, and the question is the first
// line of the body. Enter in a field sends, when every question has an answer. An answer is the
// options or the text, not both: keys type it into the dialog of the harness, which takes one.

import type { InteractionValue, QuestionAnswer } from '@neohar/transport';
import { createSignal, For, Show } from 'solid-js';
import { Badge } from '../kit/Badge';
import { Button } from '../kit/Button';
import { Choice } from '../kit/Choice';
import { Input } from '../kit/Input';
import { sendAnswer } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';
import {
  canAnswer,
  questionAnswer,
  questionComplete,
  questionDecision,
  questionTitle,
  toggled,
} from './model';

type Questions = Extract<InteractionValue['payload'], { kind: 'question' }>;

export function QuestionCard(props: {
  interaction: InteractionValue;
  payload: Questions;
  harness: string;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const host = useCardHost();
  const { report } = host;
  const questions = () => props.payload.questions;
  const single = () => questions().length === 1;
  const [selected, setSelected] = createSignal<string[][]>(questions().map(() => []));
  const [texts, setTexts] = createSignal<string[]>(questions().map(() => ''));
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const answers = (): QuestionAnswer[] =>
    questions().map((question, at) =>
      questionAnswer(question, selected()[at] ?? [], texts()[at] ?? ''),
    );
  const ready = () => questionComplete(answers(), questions().length);
  const send = async () => {
    if (busy() || !ready() || !canAnswer(props.interaction)) return;
    setBusy(true);
    setError(await sendAnswer(host, props.interaction, questionDecision(answers())));
    setBusy(false);
  };

  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error()}
      title={questionTitle(questions())}
      ref={props.ref}
    >
      <Show when={props.payload.context}>
        <p class="card-context">{props.payload.context}</p>
      </Show>
      <For each={questions()}>
        {(question, at) => (
          <fieldset class="card-ask" disabled={!canAnswer(props.interaction)}>
            {/* The header of one question is the title of the card. */}
            <legend class="card-prompt">
              <Show when={!single() && question.header}>
                <Badge>{question.header}</Badge>
              </Show>
              {question.question}
            </legend>
            <div class="card-options">
              <For each={question.options}>
                {(option) => (
                  <Choice
                    type={question.multi_select ? 'checkbox' : 'radio'}
                    name={`${props.interaction.id}-${at()}`}
                    title={option.label}
                    description={option.description}
                    checked={(selected()[at()] ?? []).includes(option.label)}
                    onChange={() => {
                      setSelected((all) =>
                        all.map((list, i) =>
                          i === at() ? toggled(list, option.label, !!question.multi_select) : list,
                        ),
                      );
                      setTexts((all) => all.map((text, i) => (i === at() ? '' : text)));
                    }}
                  />
                )}
              </For>
            </div>
            <Input
              class="card-message"
              placeholder={
                question.options.length > 0 ? 'Or type your own answer' : 'Type your answer'
              }
              aria-label={`Your own answer to: ${question.question}`}
              value={texts()[at()] ?? ''}
              onInput={(event) => {
                const value = event.currentTarget.value;
                setTexts((all) => all.map((text, i) => (i === at() ? value : text)));
                if (value.trim()) {
                  setSelected((all) => all.map((list, i) => (i === at() ? [] : list)));
                }
              }}
              onKeyDown={(event) => {
                if (event.key !== 'Enter' || event.isComposing) return;
                event.preventDefault();
                send().catch(report);
              }}
            />
          </fieldset>
        )}
      </For>
      <Show when={canAnswer(props.interaction)}>
        <div class="card-foot" role="group" aria-label="Answer">
          <Button disabled={busy() || !ready()} onClick={() => void send().catch(report)}>
            Answer
          </Button>
        </div>
      </Show>
    </CardFrame>
  );
}
