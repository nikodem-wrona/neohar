// The dialog of "Execute workflow": the folder, the base of the changes and the notes. "Take from
// agent" fills the notes with the prompts of an agent in the folder, which say what was built and
// why. The core reads the facts of the folder and starts the execution.

import type { AgentValue, WorkflowFacts } from '@neohar/transport';
import { useLayoutEffect, useRef, useState } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { useSelect } from '../lib/hooks';
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
  const dialog = useRef<HTMLDialogElement>(null);
  const first = useRef<HTMLInputElement>(null);
  const [base, setBase] = useState(props.facts?.base ?? '');
  const [notes, setNotes] = useState('');
  const panes = useSelect(stores.panes, (state) => state.byKey);
  // A layout effect: its cleanup runs before the dialog leaves the page, so `close()` can give
  // the focus back.
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    first.current?.focus();
    return () => {
      if (element?.open) element.close();
    };
  }, []);

  const agentName = (agent: AgentValue): string => {
    const pane = agent.pane ? panes[agent.pane] : undefined;
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
      className="dialog wf-execute"
      aria-labelledby="wf-execute-title"
      ref={dialog}
      onCancel={(event) => {
        event.preventDefault();
        props.onCancel();
      }}
      onMouseDown={(event) => {
        if (
          event.target === event.currentTarget &&
          outside(event.nativeEvent, event.currentTarget)
        ) {
          props.onCancel();
        }
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <form
        className="wf-execute-form"
        onSubmit={(event) => {
          event.preventDefault();
          props.onExecute(base.trim() || null, notes.trim() || null);
        }}
      >
        <h2 id="wf-execute-title" className="dialog-title">
          Execute {props.name}
        </h2>
        <p className="wf-execute-folder">
          <span className="wf-muted">Folder</span> <code>{props.folder}</code>
          {props.facts?.branch ? (
            <>
              {' '}
              <span className="wf-muted">on</span> <code>{props.facts.branch}</code>
            </>
          ) : null}
        </p>
        {!props.facts?.repo ? (
          <p className="wf-muted">
            neohar reads no git facts here: the folder is not in a git repository that you trust.
          </p>
        ) : null}
        <label className="wf-field">
          <span>Base of the changes</span>
          <input
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="main"
            value={base}
            ref={first}
            onChange={(event) => setBase(event.currentTarget.value)}
          />
        </label>
        <label className="wf-field wf-field-grow">
          <span>Notes: what you built and why</span>
          <textarea
            className="wf-notes"
            value={notes}
            onChange={(event) => setNotes(event.currentTarget.value)}
          />
        </label>
        {props.agents.length > 0 ? (
          <label className="wf-field">
            <span>Take the notes from the prompts of an agent</span>
            <select defaultValue="" onChange={(event) => take(event.currentTarget.value)}>
              <option value="">Pick an agent</option>
              {props.agents.map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agentName(agent)}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <div className="dialog-buttons">
          <button type="button" className="dialog-cancel" onClick={() => props.onCancel()}>
            Cancel
          </button>
          <button type="submit" className="primary">
            Execute
          </button>
        </div>
      </form>
    </dialog>
  );
}
