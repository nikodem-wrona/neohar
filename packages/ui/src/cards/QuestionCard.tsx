// The card of a question: the last message of the agent, each question with its options as
// choice cards, a field for an answer of the person's own, and one button that sends all the
// answers. The header of one question is the title of the card, and the question is the first
// line of the body. Enter in a field sends, when every question has an answer. An answer is the
// options or the text, not both: keys type it into the dialog of the harness, which takes one.

import type { InteractionValue, QuestionAnswer } from '@neohar/transport';
import { type Ref, useState } from 'react';
import { Badge } from '../components/ui/badge';
import { Button } from '../components/ui/button';
import { Checkbox } from '../components/ui/checkbox';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { RadioGroup, RadioGroupItem } from '../components/ui/radio-group';
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
  ref?: Ref<HTMLElement> | undefined;
}) {
  const host = useCardHost();
  const { report } = host;
  const questions = props.payload.questions;
  const single = questions.length === 1;
  const [selected, setSelected] = useState<string[][]>(() => questions.map(() => []));
  const [texts, setTexts] = useState<string[]>(() => questions.map(() => ''));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const answers: QuestionAnswer[] = questions.map((question, at) =>
    questionAnswer(question, selected[at] ?? [], texts[at] ?? ''),
  );
  const ready = questionComplete(answers, questions.length);
  const send = async () => {
    if (busy || !ready || !canAnswer(props.interaction)) return;
    setBusy(true);
    setError(await sendAnswer(host, props.interaction, questionDecision(answers)));
    setBusy(false);
  };

  /** Chooses an option of a question: its text goes. */
  const pick = (at: number, label: string, multi: boolean): void => {
    setSelected((all) => all.map((list, i) => (i === at ? toggled(list, label, multi) : list)));
    setTexts((all) => all.map((text, i) => (i === at ? '' : text)));
  };

  /**
   * The options of a question as choice cards of shadcn/ui: the title and the description, and
   * the radio button or the check box at the end. The whole card is the label, so a click
   * anywhere on it chooses.
   */
  const choices = (question: (typeof questions)[number], at: number) =>
    question.options.map((option, index) => {
      const id = `${props.interaction.id}-${at}-${index}`;
      const on = (selected[at] ?? []).includes(option.label);
      return (
        <Label
          key={option.label}
          htmlFor={id}
          className="flex cursor-pointer items-start gap-3 rounded-md border border-foreground/16 bg-background/25 px-3 py-2.5 text-[13px] leading-[18px] font-normal transition-colors hover:border-foreground/32 hover:bg-foreground/4 has-[[data-state=checked]]:border-foreground has-[[data-state=checked]]:bg-foreground/8 has-[:disabled]:cursor-default has-[:disabled]:hover:border-foreground/16 has-[:disabled]:hover:bg-background/25"
        >
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="font-semibold [overflow-wrap:anywhere] text-foreground">
              {option.label}
            </span>
            {option.description ? (
              <span className="[overflow-wrap:anywhere] text-muted-foreground">
                {option.description}
              </span>
            ) : null}
          </span>
          {question.multi_select ? (
            <Checkbox
              id={id}
              className="mt-px"
              checked={on}
              onCheckedChange={() => pick(at, option.label, true)}
            />
          ) : (
            <RadioGroupItem id={id} className="mt-px" value={option.label} />
          )}
        </Label>
      );
    });

  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error}
      title={questionTitle(questions)}
      ref={props.ref}
    >
      {props.payload.context ? <p className="card-context">{props.payload.context}</p> : null}
      {/* The questions of a card do not move: their place is their key. */}
      {questions.map((question, at) => (
        <fieldset key={at} className="card-ask" disabled={!canAnswer(props.interaction)}>
          {/* The header of one question is the title of the card. */}
          <legend className="card-prompt">
            {!single && question.header ? <Badge>{question.header}</Badge> : null}
            {question.question}
          </legend>
          {question.multi_select ? (
            <div className="card-options">{choices(question, at)}</div>
          ) : (
            <RadioGroup
              className="card-options flex flex-col gap-1.5"
              value={selected[at]?.[0] ?? ''}
              onValueChange={(label) => pick(at, label, false)}
              disabled={!canAnswer(props.interaction)}
            >
              {choices(question, at)}
            </RadioGroup>
          )}
          <Input
            className="card-message"
            placeholder={
              question.options.length > 0 ? 'Or type your own answer' : 'Type your answer'
            }
            aria-label={`Your own answer to: ${question.question}`}
            value={texts[at] ?? ''}
            onChange={(event) => {
              const value = event.currentTarget.value;
              setTexts((all) => all.map((text, i) => (i === at ? value : text)));
              if (value.trim()) {
                setSelected((all) => all.map((list, i) => (i === at ? [] : list)));
              }
            }}
            onKeyDown={(event) => {
              if (event.key !== 'Enter' || event.nativeEvent.isComposing) return;
              event.preventDefault();
              send().catch(report);
            }}
          />
        </fieldset>
      ))}
      {canAnswer(props.interaction) ? (
        <div className="card-foot" role="group" aria-label="Answer">
          <Button disabled={busy || !ready} onClick={() => void send().catch(report)}>
            Answer
          </Button>
        </div>
      ) : null}
    </CardFrame>
  );
}
