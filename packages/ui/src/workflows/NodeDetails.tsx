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
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { useLatest } from '../lib/hooks';
import { debounce } from '../lib/timing';
import { RichHtml } from '../rich/RichHtml';
import { outside } from '../shell/modal';
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
 * A select that shows a value of the node. The choice of the person stays in it until the core
 * gives the next value.
 */
function ValueSelect(props: {
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  const field = useRef<HTMLSelectElement>(null);
  useLayoutEffect(() => {
    if (field.current) field.current.value = props.value;
  }, [props.value]);
  return (
    <select ref={field} onChange={(event) => props.onChange(event.currentTarget.value)}>
      {props.children}
    </select>
  );
}

/** A check box that shows a value of the node, as `ValueSelect` does. */
function ValueCheck(props: { checked: boolean; onChange: (checked: boolean) => void }) {
  const box = useRef<HTMLInputElement>(null);
  useLayoutEffect(() => {
    if (box.current) box.current.checked = props.checked;
  }, [props.checked]);
  return (
    <input
      type="checkbox"
      ref={box}
      onChange={(event) => props.onChange(event.currentTarget.checked)}
    />
  );
}

/**
 * A field that shows a value of the node, as `ValueSelect` does. It sends the text on the native
 * `change` (on blur or Enter), not for each key.
 */
function ValueField({
  value,
  onCommit,
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
  return <input {...rest} ref={field} />;
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
  const dialog = useRef<HTMLDialogElement>(null);
  const promptField = useRef<HTMLTextAreaElement>(null);
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

  // A layout effect: its cleanup runs before the dialog leaves the page, so `close()` can give
  // the focus back.
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    const { last: execution, node } = latest.current;
    const last = execution?.nodes.find((n) => n.node === node.id);
    if (execution && last?.report) {
      transport
        .call('workflow.report', { execution: execution.id, node: node.id })
        .then((report) => setOutput(report.markdown.html))
        .catch(() => setOutputNote('The report of the last execution is gone.'));
    }
    return () => {
      if (element?.open) element.close();
    };
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
    <dialog
      className="dialog wf-details"
      aria-labelledby="wf-details-title"
      ref={dialog}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && outside(event.nativeEvent, event.currentTarget))
          close();
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <header className="wf-details-head">
        <h2 id="wf-details-title" className="dialog-title">
          {kindName(props.node.kind)} · {nodeTitle(props.node)}
        </h2>
        <span className="wf-details-id">{props.node.id}</span>
        <span className="wf-spacer" />
        <button type="button" className="dialog-cancel" onClick={close}>
          Done
        </button>
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
                  <button
                    key={chip.text}
                    type="button"
                    className="wf-chip"
                    title={chip.text}
                    disabled={props.node.kind === 'manual'}
                    onClick={() => insert(chip.text)}
                  >
                    {chip.label}
                  </button>
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
          <label className="wf-field">
            <span>Title</span>
            <input
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
          </label>
          {props.node.kind === 'agent' ? (
            <>
              <div className="wf-field-row">
                <label className="wf-field">
                  <span>Harness</span>
                  <ValueSelect
                    value={props.node.harness ?? ''}
                    onChange={(value) => {
                      props.onChange({
                        harness: value === 'claude' || value === 'codex' ? value : null,
                        model: null,
                        effort: null,
                      });
                    }}
                  >
                    <option value="claude">{harnessName('claude')}</option>
                    <option value="codex">{harnessName('codex')}</option>
                  </ValueSelect>
                </label>
                <label className="wf-field">
                  <span>Model</span>
                  <ValueSelect
                    value={props.node.model ?? ''}
                    onChange={(value) => props.onChange({ model: value || null, effort: null })}
                  >
                    <option value="">Default</option>
                    {(choice?.models ?? []).map((model) => (
                      <option key={model.id} value={model.id}>
                        {model.label}
                      </option>
                    ))}
                    {props.node.model &&
                    !(choice?.models ?? []).some((m) => m.id === props.node.model) ? (
                      <option value={props.node.model ?? ''}>{props.node.model}</option>
                    ) : null}
                  </ValueSelect>
                </label>
                <label className="wf-field">
                  <span>Effort</span>
                  <ValueSelect
                    value={props.node.effort ?? ''}
                    onChange={(value) => props.onChange({ effort: value || null })}
                  >
                    <option value="">Default</option>
                    {efforts.map((effort) => (
                      <option key={effort} value={effort}>
                        {effort}
                      </option>
                    ))}
                    {props.node.effort && !efforts.includes(props.node.effort) ? (
                      <option value={props.node.effort ?? ''}>{props.node.effort}</option>
                    ) : null}
                  </ValueSelect>
                </label>
              </div>
              <div className="wf-field-row">
                <label className="wf-check">
                  <ValueCheck
                    checked={props.node.can_edit === true}
                    onChange={(checked) => props.onChange({ can_edit: checked })}
                  />
                  <span>Can edit files</span>
                </label>
                <label className="wf-field wf-field-small">
                  <span>Time limit, min</span>
                  <ValueField
                    type="number"
                    min="1"
                    max="1440"
                    placeholder="60"
                    value={String(props.node.time_limit_min ?? '')}
                    onCommit={onLimit}
                  />
                </label>
              </div>
            </>
          ) : null}
          {props.node.kind === 'report' ? (
            <label className="wf-field">
              <span>Summary by</span>
              <ValueSelect
                value={props.node.summary_by ?? ''}
                onChange={(value) => {
                  props.onChange({
                    summary_by: value === 'claude' || value === 'codex' ? value : null,
                  });
                }}
              >
                <option value="">No summary</option>
                <option value="claude">{harnessName('claude')}</option>
                <option value="codex">{harnessName('codex')}</option>
              </ValueSelect>
            </label>
          ) : null}
          {props.node.kind === 'agent' ||
          (props.node.kind === 'report' && props.node.summary_by) ? (
            <>
              <label className="wf-field wf-field-grow">
                <span>{props.node.kind === 'report' ? 'Summary prompt (optional)' : 'Prompt'}</span>
                <textarea
                  className="wf-prompt"
                  spellCheck={false}
                  value={prompt}
                  ref={promptField}
                  onChange={(event) => {
                    setPrompt(event.currentTarget.value);
                    savePrompt(event.currentTarget.value);
                  }}
                  onBlur={flush}
                />
              </label>
              <p className="wf-muted wf-footer-note">
                neohar adds a footer to the prompt: the paths of the input reports, and the rules.
                The agent asks no questions
                {props.node.can_edit || props.node.kind === 'report' ? '' : ', edits no files'} and
                ends its turn with its full report in Markdown
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
            <button
              type="button"
              className="wf-secondary"
              title="Execute only this node, with the reports of the last execution as its inputs"
              onClick={() => {
                flush();
                props.onExecuteNode();
              }}
            >
              Execute this node
            </button>
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
    </dialog>
  );
}
