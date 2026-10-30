// The dialog of "Execute workflow": the folder, the base of the changes and the notes. "Take from
// agent" fills the notes with the prompts of an agent in the folder, which say what was built and
// why. The core reads the facts of the folder and starts the execution.

import type { AgentValue, WorkflowFacts } from '@neohar/transport';
import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { outside } from '../shell/modal';

export function ExecuteDialog(props: {
  workflow: string;
  name: string;
  folder: string;
  facts: WorkflowFacts | null;
  /** The agents of the board, for "Take from agent". */
  agents: readonly AgentValue[];
  onExecute(base: string | null, notes: string | null): void;
  onCancel(): void;
}) {
  const { transport, stores, report } = useApp();
  let dialog: HTMLDialogElement | undefined;
  let first: HTMLInputElement | undefined;
  const [base, setBase] = createSignal(props.facts?.base ?? '');
  const [notes, setNotes] = createSignal('');
  onMount(() => {
    dialog?.showModal();
    first?.focus();
  });
  onCleanup(() => {
    if (dialog?.open) dialog.close();
  });

  const agentName = (agent: AgentValue): string => {
    const pane = agent.pane ? stores.panes.state.byKey[agent.pane] : undefined;
    const title = pane?.title?.trim();
    return title ? `${harnessName(agent.harness)} · ${title}` : harnessName(agent.harness);
  };
  const take = (agent: string): void => {
    if (!agent) return;
    transport
      .call('workflow.notes_from', { agent })
      .then((found) => setNotes(found.text))
      .catch(report);
  };

  return (
    <dialog
      class="dialog wf-execute"
      aria-labelledby="wf-execute-title"
      ref={(element) => (dialog = element)}
      onCancel={(event) => {
        event.preventDefault();
        props.onCancel();
      }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && outside(event, event.currentTarget)) {
          props.onCancel();
        }
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <form
        class="wf-execute-form"
        onSubmit={(event) => {
          event.preventDefault();
          props.onExecute(base().trim() || null, notes().trim() || null);
        }}
      >
        <h2 id="wf-execute-title" class="dialog-title">
          Execute {props.name}
        </h2>
        <p class="wf-execute-folder">
          <span class="wf-muted">Folder</span> <code>{props.folder}</code>
          <Show when={props.facts?.branch}>
            {(branch) => (
              <>
                {' '}
                <span class="wf-muted">on</span> <code>{branch()}</code>
              </>
            )}
          </Show>
        </p>
        <Show when={!props.facts?.repo}>
          <p class="wf-muted">
            neohar reads no git facts here: the folder is not in a git repository that you trust.
          </p>
        </Show>
        <label class="wf-field">
          <span>Base of the changes</span>
          <input
            type="text"
            autocomplete="off"
            spellcheck={false}
            placeholder="main"
            value={base()}
            ref={(element) => (first = element)}
            onInput={(event) => setBase(event.currentTarget.value)}
          />
        </label>
        <label class="wf-field wf-field-grow">
          <span>Notes: what you built and why</span>
          <textarea
            class="wf-notes"
            value={notes()}
            onInput={(event) => setNotes(event.currentTarget.value)}
          />
        </label>
        <Show when={props.agents.length > 0}>
          <label class="wf-field">
            <span>Take the notes from the prompts of an agent</span>
            <select value="" onChange={(event) => take(event.currentTarget.value)}>
              <option value="">Pick an agent</option>
              <For each={props.agents}>
                {(agent) => <option value={agent.id}>{agentName(agent)}</option>}
              </For>
            </select>
          </label>
        </Show>
        <div class="dialog-buttons">
          <button type="button" class="dialog-cancel" onClick={() => props.onCancel()}>
            Cancel
          </button>
          <button type="submit" class="primary">
            Execute
          </button>
        </div>
      </form>
    </dialog>
  );
}
