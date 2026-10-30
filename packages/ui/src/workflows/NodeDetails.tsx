// The details of a node, as in n8n: the inputs on the left, the settings in the middle and the
// last output on the right. A double-click on a card opens them.
//
// A click on a chip of the inputs puts its reference into the prompt at the cursor. The text fields
// save 500 ms after the last key and on blur; the other fields save at once. The core validates the
// graph and gives back the errors of the node.

import type {
  ExecutionValue,
  HarnessChoices,
  WorkflowGraph,
  WorkflowNode,
  WorkflowProblem,
} from '@neohar/transport';
import {
  type ComponentProps,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import { Button } from '../components/ui/button';
import { Checkbox } from '../components/ui/checkbox';
import { Dialog, DialogContent, DialogTitle } from '../components/ui/dialog';
import { Field, FieldLabel } from '../components/ui/field';
import { Input } from '../components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { Textarea } from '../components/ui/textarea';
import { useLatest } from '../lib/hooks';
import { debounce } from '../lib/timing';
import { cn } from '../lib/utils';
import { RichHtml } from '../rich/RichHtml';
import { LABEL, NONE } from './fields';
import {
  harnessName,
  inputsOf,
  insertAt,
  kindName,
  nodeTitle,
  problemsOf,
  referenceChips,
  runLine,
} from './model';

/**
 * A select that shows a value of the node. The choice of the person shows at once and stays until
 * the core gives the next value. The value `''` is the option "none".
 */
function ValueSelect(props: {
  id: string;
  value: string;
  options: readonly { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  const [shown, setShown] = useState({ core: props.value, value: props.value });
  if (shown.core !== props.value) setShown({ core: props.value, value: props.value });
  return (
    <Select
      value={shown.value || NONE}
      onValueChange={(next) => {
        const value = next === NONE ? '' : next;
        setShown({ core: props.value, value });
        props.onChange(value);
      }}
    >
      <SelectTrigger id={props.id} size="sm" className="w-full text-[13px]">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {props.options.map((option) => (
          <SelectItem key={option.value} value={option.value || NONE} className="text-[13px]">
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** A check box that shows a value of the node, as `ValueSelect` does. */
function ValueCheck(props: { id: string; checked: boolean; onChange: (checked: boolean) => void }) {
  const [shown, setShown] = useState({ core: props.checked, on: props.checked });
  if (shown.core !== props.checked) setShown({ core: props.checked, on: props.checked });
  return (
    <Checkbox
      id={props.id}
      checked={shown.on}
      onCheckedChange={(state) => {
        const on = state === true;
        setShown({ core: props.checked, on });
        props.onChange(on);
      }}
    />
  );
}

/**
 * A field that shows a value of the node. The person edits it freely: it sends the text on the
 * native `change` (on blur or Enter), not for each key, and a new value of the core goes into it.
 */
function ValueField({
  value,
  onCommit,
  className,
  ...rest
}: { value: string; onCommit: (text: string) => void } & Omit<
  ComponentProps<'input'>,
  'value' | 'onChange' | 'ref'
>) {
  const field = useRef<HTMLInputElement>(null);
  const commit = useLatest(onCommit);
  useLayoutEffect(() => {
    if (field.current) field.current.value = value;
  }, [value]);
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    const change = (): void => commit.current(element.value);
    element.addEventListener('change', change);
    return () => element.removeEventListener('change', change);
  }, [commit]);
  return <Input {...rest} className={cn('h-8 md:text-[13px]', className)} ref={field} />;
}

export interface NodeDetailsProps {
  graph: WorkflowGraph;
  node: WorkflowNode;
  problems: readonly WorkflowProblem[];
  choices: readonly HarnessChoices[];
  /** The last execution of the workflow in this folder, for the reports of before. */
  last?: ExecutionValue | undefined;
  onChange(patch: Partial<WorkflowNode>): void;
  onExecuteNode(): void;
  onClose(): void;
}

export function NodeDetails(props: NodeDetailsProps) {
  const { transport } = useApp();
  const promptField = useRef<HTMLTextAreaElement>(null);
  // The ids that tie each label to its field.
  const id = useId();
  const [prompt, setPrompt] = useState(() =>
    props.node.kind === 'report' ? (props.node.summary_prompt ?? '') : (props.node.prompt ?? ''),
  );
  const [title, setTitle] = useState(props.node.title ?? '');
  const [output, setOutput] = useState<string | null>(null);
  const [outputNote, setOutputNote] = useState(
    'No execution in this folder has a report of this node yet.',
  );
  // The saves that wait read the node and the callback of now.
  const latest = useLatest(props);

  const [savePrompt] = useState(() =>
    debounce((text: string) => {
      const { node, onChange } = latest.current;
      if (node.kind === 'report') onChange({ summary_prompt: text.trim() ? text : null });
      else onChange({ prompt: text });
    }, 500),
  );
  const [saveTitle] = useState(() =>
    debounce((text: string) => latest.current.onChange({ title: text }), 500),
  );
  const flush = (): void => {
    savePrompt.cancel();
    saveTitle.cancel();
    const kept =
      props.node.kind === 'report' ? (props.node.summary_prompt ?? '') : (props.node.prompt ?? '');
    if (prompt !== kept && props.node.kind !== 'manual') {
      if (props.node.kind === 'report')
        props.onChange({ summary_prompt: prompt.trim() ? prompt : null });
      else props.onChange({ prompt });
    }
    if (title !== (props.node.title ?? '')) props.onChange({ title });
  };
  const close = (): void => {
    flush();
    props.onClose();
  };

  // The dialog is open while it is on the page. When it closes, the focus goes back to the
  // element that had it, usually the canvas.
  useLayer(true);
  const [before] = useState(() => document.activeElement);
  useEffect(() => {
    const { last: execution, node } = latest.current;
    const last = execution?.nodes.find((n) => n.node === node.id);
    if (execution && last?.report) {
      transport
        .call('workflow.report', { execution: execution.id, node: node.id })
        .then((report) => setOutput(report.markdown.html))
        .catch(() => setOutputNote('The report of the last execution is gone.'));
    }
  }, [latest, transport]);

  const choice = props.choices.find((c) => c.harness === props.node.harness);
  const model = choice?.models.find((m) => m.id === props.node.model);
  const efforts = model?.efforts?.length ? model.efforts : (choice?.efforts ?? []);
  const insert = (text: string): void => {
    const field = promptField.current;
    const start = field?.selectionStart ?? prompt.length;
    const end = field?.selectionEnd ?? prompt.length;
    const next = insertAt(prompt, start, end, text);
    setPrompt(next.text);
    savePrompt(next.text);
    queueMicrotask(() => {
      field?.focus();
      field?.setSelectionRange(next.at, next.at);
    });
  };
  const inputs = inputsOf(props.node)
    .map((id) => props.graph.nodes.find((n) => n.id === id))
    .filter((n): n is WorkflowNode => n !== undefined && n.kind !== 'manual');
  const lastOf = (id: string) => props.last?.nodes.find((n) => n.node === id);
  const issues = problemsOf(props.problems, props.node.id);
  const onLimit = useCallback(
    (text: string) => {
      const value = Number.parseInt(text, 10);
      latest.current.onChange({ time_limit_min: Number.isFinite(value) ? value : null });
    },
    [latest],
  );

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        className="flex h-[calc(88vh-16px)] w-[min(150ch,calc(100vw-32px))] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
        showCloseButton={false}
        aria-describedby={undefined}
        // The dialog has no trigger: Radix gives the focus to none, so the dialog gives it back.
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (before instanceof HTMLElement) before.focus();
        }}
        // The keys of the dialog do not reach the view and the canvas under it.
        onKeyDown={(event) => event.stopPropagation()}
      >
        <header className="wf-details-head">
          <DialogTitle className="dialog-title text-[length:inherit] leading-[inherit]">
            {kindName(props.node.kind)} · {nodeTitle(props.node)}
          </DialogTitle>
          <span className="wf-details-id">{props.node.id}</span>
          <span className="wf-spacer" />
          <Button variant="outline" size="sm" onClick={close}>
            Done
          </Button>
        </header>
        {issues.length > 0 ? (
          <ul className="wf-details-problems" role="alert">
            {issues.map((issue, index) => (
              // A problem has no id, and the list is new with each answer of the core.
              <li key={index}>{issue.text}</li>
            ))}
          </ul>
        ) : null}
        <div className="wf-details-body">
          <section className="wf-details-col" aria-label="Input">
            <h3>Input</h3>
            {props.node.kind !== 'manual' ? (
              <>
                <p className="wf-muted">Click a chip to put its reference into the prompt.</p>
                <div className="wf-chips">
                  {referenceChips(props.graph, props.node).map((chip) => (
                    <Button
                      key={chip.text}
                      variant="outline"
                      size="xs"
                      className="rounded-full font-mono"
                      title={chip.text}
                      disabled={props.node.kind === 'manual'}
                      onClick={() => insert(chip.text)}
                    >
                      {chip.label}
                    </Button>
                  ))}
                </div>
                <ul className="wf-inputs">
                  {inputs.map((input) => (
                    <li key={input.id}>
                      <span className="wf-input-title">{nodeTitle(input)}</span>
                      <span className="wf-muted">
                        {lastOf(input.id) ? runLine(lastOf(input.id)) : 'no report yet'}
                      </span>
                    </li>
                  ))}
                </ul>
              </>
            ) : (
              <p className="wf-muted">
                The trigger has no input. It gives the facts of an execution to the nodes after it.
              </p>
            )}
          </section>
          <section className="wf-details-col wf-details-settings" aria-label="Settings">
            <h3>Settings</h3>
            <Field className="wf-field gap-0.5">
              <FieldLabel htmlFor={`${id}-title`} className={LABEL}>
                Title
              </FieldLabel>
              <Input
                id={`${id}-title`}
                className="h-8 md:text-[13px]"
                type="text"
                autoComplete="off"
                spellCheck={false}
                value={title}
                onChange={(event) => {
                  setTitle(event.currentTarget.value);
                  saveTitle(event.currentTarget.value);
                }}
                onBlur={flush}
              />
            </Field>
            {props.node.kind === 'agent' ? (
              <>
                <div className="wf-field-row">
                  <Field className="wf-field gap-0.5">
                    <FieldLabel htmlFor={`${id}-harness`} className={LABEL}>
                      Harness
                    </FieldLabel>
                    <ValueSelect
                      id={`${id}-harness`}
                      value={props.node.harness ?? ''}
                      options={[
                        { value: 'claude', label: harnessName('claude') },
                        { value: 'codex', label: harnessName('codex') },
                      ]}
                      onChange={(value) => {
                        props.onChange({
                          harness: value === 'claude' || value === 'codex' ? value : null,
                          model: null,
                          effort: null,
                        });
                      }}
                    />
                  </Field>
                  <Field className="wf-field gap-0.5">
                    <FieldLabel htmlFor={`${id}-model`} className={LABEL}>
                      Model
                    </FieldLabel>
                    <ValueSelect
                      id={`${id}-model`}
                      value={props.node.model ?? ''}
                      options={[
                        { value: '', label: 'Default' },
                        ...(choice?.models ?? []).map((model) => ({
                          value: model.id,
                          label: model.label,
                        })),
                        ...(props.node.model &&
                        !(choice?.models ?? []).some((m) => m.id === props.node.model)
                          ? [{ value: props.node.model, label: props.node.model }]
                          : []),
                      ]}
                      onChange={(value) => props.onChange({ model: value || null, effort: null })}
                    />
                  </Field>
                  <Field className="wf-field gap-0.5">
                    <FieldLabel htmlFor={`${id}-effort`} className={LABEL}>
                      Effort
                    </FieldLabel>
                    <ValueSelect
                      id={`${id}-effort`}
                      value={props.node.effort ?? ''}
                      options={[
                        { value: '', label: 'Default' },
                        ...efforts.map((effort) => ({ value: effort, label: effort })),
                        ...(props.node.effort && !efforts.includes(props.node.effort)
                          ? [{ value: props.node.effort, label: props.node.effort }]
                          : []),
                      ]}
                      onChange={(value) => props.onChange({ effort: value || null })}
                    />
                  </Field>
                </div>
                <div className="wf-field-row">
                  <Field orientation="horizontal" className="w-auto gap-2 pb-2">
                    <ValueCheck
                      id={`${id}-edit`}
                      checked={props.node.can_edit === true}
                      onChange={(checked) => props.onChange({ can_edit: checked })}
                    />
                    <FieldLabel htmlFor={`${id}-edit`} className="text-[13px] font-normal">
                      Can edit files
                    </FieldLabel>
                  </Field>
                  <Field className="wf-field wf-field-small gap-0.5">
                    <FieldLabel htmlFor={`${id}-limit`} className={LABEL}>
                      Time limit, min
                    </FieldLabel>
                    <ValueField
                      id={`${id}-limit`}
                      type="number"
                      min="1"
                      max="1440"
                      placeholder="60"
                      value={String(props.node.time_limit_min ?? '')}
                      onCommit={onLimit}
                    />
                  </Field>
                </div>
              </>
            ) : null}
            {props.node.kind === 'report' ? (
              <Field className="wf-field gap-0.5">
                <FieldLabel htmlFor={`${id}-summary`} className={LABEL}>
                  Summary by
                </FieldLabel>
                <ValueSelect
                  id={`${id}-summary`}
                  value={props.node.summary_by ?? ''}
                  options={[
                    { value: '', label: 'No summary' },
                    { value: 'claude', label: harnessName('claude') },
                    { value: 'codex', label: harnessName('codex') },
                  ]}
                  onChange={(value) => {
                    props.onChange({
                      summary_by: value === 'claude' || value === 'codex' ? value : null,
                    });
                  }}
                />
              </Field>
            ) : null}
            {props.node.kind === 'agent' ||
            (props.node.kind === 'report' && props.node.summary_by) ? (
              <>
                <Field className="wf-field wf-field-grow gap-0.5">
                  <FieldLabel htmlFor={`${id}-prompt`} className={LABEL}>
                    {props.node.kind === 'report' ? 'Summary prompt (optional)' : 'Prompt'}
                  </FieldLabel>
                  <Textarea
                    id={`${id}-prompt`}
                    className="min-h-[10em] flex-1 resize-y font-mono leading-[1.45] [field-sizing:fixed] md:text-[length:var(--size-data)]"
                    spellCheck={false}
                    value={prompt}
                    ref={promptField}
                    onChange={(event) => {
                      setPrompt(event.currentTarget.value);
                      savePrompt(event.currentTarget.value);
                    }}
                    onBlur={flush}
                  />
                </Field>
                <p className="wf-muted wf-footer-note">
                  neohar adds a footer to the prompt: the paths of the input reports, and the rules.
                  The agent asks no questions
                  {props.node.can_edit || props.node.kind === 'report'
                    ? ''
                    : ', edits no files'}{' '}
                  and ends its turn with its full report in Markdown
                  {props.node.kind === 'report'
                    ? '.'
                    : ', with a last line "Verdict: pass", "warn" or "fail".'}
                </p>
              </>
            ) : null}
            {props.node.kind === 'report' && !props.node.summary_by ? (
              <p className="wf-muted">
                The node writes report.md: the facts of the execution, a table of the inputs with
                their verdicts, and a link to each report.
              </p>
            ) : null}
          </section>
          <section className="wf-details-col" aria-label="Output">
            <h3>Output</h3>
            {props.node.kind !== 'manual' ? (
              <Button
                variant="outline"
                size="sm"
                className="self-start"
                title="Execute only this node, with the reports of the last execution as its inputs"
                onClick={() => {
                  flush();
                  props.onExecuteNode();
                }}
              >
                Execute this node
              </Button>
            ) : null}
            {output ? (
              <div className="wf-report">
                <RichHtml html={output} />
              </div>
            ) : (
              <p className="wf-muted">{outputNote}</p>
            )}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
