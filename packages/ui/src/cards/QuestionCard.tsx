// The card of a question: the last message of the agent, each question with its
// options and their descriptions, a free text, and one button that sends all the answers. An
// answer is the options or the text, not both: keys type it into the dialog of the harness,
// which takes one.

import type { InteractionValue, QuestionAnswer } from '@neohar/transport';
import { createSignal, For, Show } from 'solid-js';
import { sendAnswer } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';
import { canAnswer, questionAnswer, questionComplete, questionDecision, toggled } from './model';

type Questions = Extract<InteractionValue['payload'], { kind: 'question' }>;

export function QuestionCard(props: {
  interaction: InteractionValue;
  payload: Questions;
  harness: string;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const { transport, via, report } = useCardHost();
  const questions = () => props.payload.questions;
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
    setError(await sendAnswer(transport, props.interaction, questionDecision(answers()), via));
    setBusy(false);
  };

  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error()}
      ref={props.ref}
    >
      <Show when={props.payload.context}>
        <p class="card-context">{props.payload.context}</p>
      </Show>
      <For each={questions()}>
        {(question, at) => (
          <fieldset class="card-question" disabled={!canAnswer(props.interaction)}>
            <legend>
              <Show when={question.header}>
                <span class="card-label">{question.header}</span>{' '}
              </Show>
              {question.question}
            </legend>
            <For each={question.options}>
              {(option) => (
                <label class="card-option">
                  <input
                    type={question.multi_select ? 'checkbox' : 'radio'}
                    name={`${props.interaction.id}-${at()}`}
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
                  <span class="card-option-label">{option.label}</span>
                  <Show when={option.description}>
                    <span class="card-option-description">{option.description}</span>
                  </Show>
                </label>
              )}
            </For>
            <input
              class="card-message"
              type="text"
              placeholder="Or type an answer"
              aria-label={`Your own answer to: ${question.question}`}
              value={texts()[at()] ?? ''}
              onInput={(event) => {
                const value = event.currentTarget.value;
                setTexts((all) => all.map((text, i) => (i === at() ? value : text)));
                if (value.trim()) {
                  setSelected((all) => all.map((list, i) => (i === at() ? [] : list)));
                }
              }}
            />
          </fieldset>
        )}
      </For>
      <Show when={canAnswer(props.interaction)}>
        <div class="card-actions" role="group" aria-label="Answer">
          <button
            type="button"
            class="card-button selected"
            disabled={busy() || !ready()}
            onClick={() => void send().catch(report)}
          >
            Answer
          </button>
        </div>
      </Show>
    </CardFrame>
  );
}
