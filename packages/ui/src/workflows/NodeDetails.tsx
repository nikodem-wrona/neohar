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
import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
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
  let dialog: HTMLDialogElement | undefined;
  let promptField: HTMLTextAreaElement | undefined;
  const [prompt, setPrompt] = createSignal(
    props.node.kind === 'report' ? (props.node.summary_prompt ?? '') : (props.node.prompt ?? ''),
  );
  const [title, setTitle] = createSignal(props.node.title ?? '');
  const [output, setOutput] = createSignal<string | null>(null);
  const [outputNote, setOutputNote] = createSignal(
    'No execution in this folder has a report of this node yet.',
  );

  const savePrompt = debounce((text: string) => {
    if (props.node.kind === 'report') props.onChange({ summary_prompt: text.trim() ? text : null });
    else props.onChange({ prompt: text });
  }, 500);
  const saveTitle = debounce((text: string) => props.onChange({ title: text }), 500);
  const flush = (): void => {
    savePrompt.cancel();
    saveTitle.cancel();
    const kept =
      props.node.kind === 'report' ? (props.node.summary_prompt ?? '') : (props.node.prompt ?? '');
    if (prompt() !== kept && props.node.kind !== 'manual') {
      if (props.node.kind === 'report')
        props.onChange({ summary_prompt: prompt().trim() ? prompt() : null });
      else props.onChange({ prompt: prompt() });
    }
    if (title() !== (props.node.title ?? '')) props.onChange({ title: title() });
  };
  const close = (): void => {
    flush();
    props.onClose();
  };

  onMount(() => {
    dialog?.showModal();
    const last = props.last?.nodes.find((n) => n.node === props.node.id);
    if (props.last && last?.report) {
      transport
        .call('workflow.report', { execution: props.last.id, node: props.node.id })
        .then((report) => setOutput(report.markdown.html))
        .catch(() => setOutputNote('The report of the last execution is gone.'));
    }
  });
  onCleanup(() => {
    if (dialog?.open) dialog.close();
  });

  const choice = () => props.choices.find((c) => c.harness === props.node.harness);
  const efforts = () => {
    const model = choice()?.models.find((m) => m.id === props.node.model);
    return model?.efforts?.length ? model.efforts : (choice()?.efforts ?? []);
  };
  const insert = (text: string): void => {
    const field = promptField;
    const start = field?.selectionStart ?? prompt().length;
    const end = field?.selectionEnd ?? prompt().length;
    const next = insertAt(prompt(), start, end, text);
    setPrompt(next.text);
    savePrompt(next.text);
    queueMicrotask(() => {
      field?.focus();
      field?.setSelectionRange(next.at, next.at);
    });
  };
  const inputs = () =>
    inputsOf(props.node)
      .map((id) => props.graph.nodes.find((n) => n.id === id))
      .filter((n): n is WorkflowNode => n !== undefined && n.kind !== 'manual');
  const lastOf = (id: string) => props.last?.nodes.find((n) => n.node === id);
  const issues = () => problemsOf(props.problems, props.node.id);

  return (
    <dialog
      class="dialog wf-details"
      aria-labelledby="wf-details-title"
      ref={(element) => (dialog = element)}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && outside(event, event.currentTarget)) close();
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <header class="wf-details-head">
        <h2 id="wf-details-title" class="dialog-title">
          {kindName(props.node.kind)} · {nodeTitle(props.node)}
        </h2>
        <span class="wf-details-id">{props.node.id}</span>
        <span class="wf-spacer" />
        <button type="button" class="dialog-cancel" onClick={close}>
          Done
        </button>
      </header>
      <Show when={issues().length > 0}>
        <ul class="wf-details-problems" role="alert">
          <For each={issues()}>{(issue) => <li>{issue.text}</li>}</For>
        </ul>
      </Show>
      <div class="wf-details-body">
        <section class="wf-details-col" aria-label="Input">
          <h3>Input</h3>
          <Show
            when={props.node.kind !== 'manual'}
            fallback={
              <p class="wf-muted">
                The trigger has no input. It gives the facts of an execution to the nodes after it.
              </p>
            }
          >
            <p class="wf-muted">Click a chip to put its reference into the prompt.</p>
            <div class="wf-chips">
              <For each={referenceChips(props.graph, props.node)}>
                {(chip) => (
                  <button
                    type="button"
                    class="wf-chip"
                    title={chip.text}
                    disabled={props.node.kind === 'manual'}
                    onClick={() => insert(chip.text)}
                  >
                    {chip.label}
                  </button>
                )}
              </For>
            </div>
            <ul class="wf-inputs">
              <For each={inputs()}>
                {(input) => (
                  <li>
                    <span class="wf-input-title">{nodeTitle(input)}</span>
                    <span class="wf-muted">
                      {lastOf(input.id) ? runLine(lastOf(input.id)) : 'no report yet'}
                    </span>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </section>
        <section class="wf-details-col wf-details-settings" aria-label="Settings">
          <h3>Settings</h3>
          <label class="wf-field">
            <span>Title</span>
            <input
              type="text"
              autocomplete="off"
              spellcheck={false}
              value={title()}
              onInput={(event) => {
                setTitle(event.currentTarget.value);
                saveTitle(event.currentTarget.value);
              }}
              onBlur={flush}
            />
          </label>
          <Show when={props.node.kind === 'agent'}>
            <div class="wf-field-row">
              <label class="wf-field">
                <span>Harness</span>
                <select
                  value={props.node.harness ?? ''}
                  onChange={(event) => {
                    const value = event.currentTarget.value;
                    props.onChange({
                      harness: value === 'claude' || value === 'codex' ? value : null,
                      model: null,
                      effort: null,
                    });
                  }}
                >
                  <option value="claude">{harnessName('claude')}</option>
                  <option value="codex">{harnessName('codex')}</option>
                </select>
              </label>
              <label class="wf-field">
                <span>Model</span>
                <select
                  value={props.node.model ?? ''}
                  onChange={(event) =>
                    props.onChange({ model: event.currentTarget.value || null, effort: null })
                  }
                >
                  <option value="">Default</option>
                  <For each={choice()?.models ?? []}>
                    {(model) => <option value={model.id}>{model.label}</option>}
                  </For>
                  <Show
                    when={
                      props.node.model &&
                      !(choice()?.models ?? []).some((m) => m.id === props.node.model)
                    }
                  >
                    <option value={props.node.model ?? ''}>{props.node.model}</option>
                  </Show>
                </select>
              </label>
              <label class="wf-field">
                <span>Effort</span>
                <select
                  value={props.node.effort ?? ''}
                  onChange={(event) =>
                    props.onChange({ effort: event.currentTarget.value || null })
                  }
                >
                  <option value="">Default</option>
                  <For each={efforts()}>{(effort) => <option value={effort}>{effort}</option>}</For>
                  <Show when={props.node.effort && !efforts().includes(props.node.effort)}>
                    <option value={props.node.effort ?? ''}>{props.node.effort}</option>
                  </Show>
                </select>
              </label>
            </div>
            <div class="wf-field-row">
              <label class="wf-check">
                <input
                  type="checkbox"
                  checked={props.node.can_edit === true}
                  onChange={(event) => props.onChange({ can_edit: event.currentTarget.checked })}
                />
                <span>Can edit files</span>
              </label>
              <label class="wf-field wf-field-small">
                <span>Time limit, min</span>
                <input
                  type="number"
                  min="1"
                  max="1440"
                  placeholder="60"
                  value={props.node.time_limit_min ?? ''}
                  onChange={(event) => {
                    const value = Number.parseInt(event.currentTarget.value, 10);
                    props.onChange({ time_limit_min: Number.isFinite(value) ? value : null });
                  }}
                />
              </label>
            </div>
          </Show>
          <Show when={props.node.kind === 'report'}>
            <label class="wf-field">
              <span>Summary by</span>
              <select
                value={props.node.summary_by ?? ''}
                onChange={(event) => {
                  const value = event.currentTarget.value;
                  props.onChange({
                    summary_by: value === 'claude' || value === 'codex' ? value : null,
                  });
                }}
              >
                <option value="">No summary</option>
                <option value="claude">{harnessName('claude')}</option>
                <option value="codex">{harnessName('codex')}</option>
              </select>
            </label>
          </Show>
          <Show
            when={
              props.node.kind === 'agent' || (props.node.kind === 'report' && props.node.summary_by)
            }
          >
            <label class="wf-field wf-field-grow">
              <span>{props.node.kind === 'report' ? 'Summary prompt (optional)' : 'Prompt'}</span>
              <textarea
                class="wf-prompt"
                spellcheck={false}
                value={prompt()}
                ref={(element) => (promptField = element)}
                onInput={(event) => {
                  setPrompt(event.currentTarget.value);
                  savePrompt(event.currentTarget.value);
                }}
                onBlur={flush}
              />
            </label>
            <p class="wf-muted wf-footer-note">
              neohar adds a footer to the prompt: the paths of the input reports, and the rules. The
              agent asks no questions
              {props.node.can_edit || props.node.kind === 'report' ? '' : ', edits no files'} and
              ends its turn with its full report in Markdown
              {props.node.kind === 'report'
                ? '.'
                : ', with a last line "Verdict: pass", "warn" or "fail".'}
            </p>
          </Show>
          <Show when={props.node.kind === 'report' && !props.node.summary_by}>
            <p class="wf-muted">
              The node writes report.md: the facts of the execution, a table of the inputs with
              their verdicts, and a link to each report.
            </p>
          </Show>
        </section>
        <section class="wf-details-col" aria-label="Output">
          <h3>Output</h3>
          <Show when={props.node.kind !== 'manual'}>
            <button
              type="button"
              class="wf-secondary"
              title="Execute only this node, with the reports of the last execution as its inputs"
              onClick={() => {
                flush();
                props.onExecuteNode();
              }}
            >
              Execute this node
            </button>
          </Show>
          <Show when={output()} fallback={<p class="wf-muted">{outputNote()}</p>}>
            {(html) => (
              <div class="wf-report">
                <RichHtml html={html()} />
              </div>
            )}
          </Show>
        </section>
      </div>
    </dialog>
  );
}
