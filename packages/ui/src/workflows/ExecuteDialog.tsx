// The dialog of "Execute workflow": the folder, the base of the changes and the notes. "Take from
// agent" fills the notes with the prompts of an agent in the folder, which say what was built and
// why. The core reads the facts of the folder and starts the execution.

import type { AgentValue, WorkflowFacts } from '@neohar/transport';
import { useId, useRef, useState } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import { Button } from '../components/ui/button';
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
import { useSelect } from '../lib/hooks';
import { LABEL } from './fields';

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
  const first = useRef<HTMLInputElement>(null);
  // The ids that tie each label to its field.
  const id = useId();
  const [base, setBase] = useState(props.facts?.base ?? '');
  const [notes, setNotes] = useState('');
  const panes = useSelect(stores.panes, (state) => state.byKey);
  // The dialog is open while it is on the page. When it closes, the focus goes back to the
  // element that had it, usually the button Execute workflow.
  useLayer(true);
  const [before] = useState(() => document.activeElement);

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
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onCancel();
      }}
    >
      <DialogContent
        className="block max-h-[calc(92vh-16px)] w-[min(72ch,calc(100vw-32px))] max-w-none overflow-auto p-0 sm:max-w-none"
        showCloseButton={false}
        aria-describedby={undefined}
        // The dialog has no trigger: Radix gives the focus to none, so the dialog gives it back.
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (before instanceof HTMLElement) before.focus();
        }}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          first.current?.focus();
        }}
        // The keys of the dialog do not reach the view and the canvas under it.
        onKeyDown={(event) => event.stopPropagation()}
      >
        <form
          className="wf-execute-form"
          onSubmit={(event) => {
            event.preventDefault();
            props.onExecute(base.trim() || null, notes.trim() || null);
          }}
        >
          <DialogTitle className="dialog-title text-sm">Execute {props.name}</DialogTitle>
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
            <p className="wf-muted m-0">
              neohar reads no git facts here: the folder is not in a git repository that you trust.
            </p>
          ) : null}
          <Field className="wf-field gap-0.5">
            <FieldLabel htmlFor={`${id}-base`} className={LABEL}>
              Base of the changes
            </FieldLabel>
            <Input
              id={`${id}-base`}
              className="h-8"
              type="text"
              autoComplete="off"
              spellCheck={false}
              placeholder="main"
              value={base}
              ref={first}
              onChange={(event) => setBase(event.currentTarget.value)}
            />
          </Field>
          <Field className="wf-field wf-field-grow gap-0.5">
            <FieldLabel htmlFor={`${id}-notes`} className={LABEL}>
              Notes: what you built and why
            </FieldLabel>
            <Textarea
              id={`${id}-notes`}
              className="min-h-[10em] flex-1 resize-y font-mono leading-[1.45] [field-sizing:fixed] md:text-[length:var(--size-data)]"
              value={notes}
              onChange={(event) => setNotes(event.currentTarget.value)}
            />
          </Field>
          {props.agents.length > 0 ? (
            <Field className="wf-field gap-0.5">
              <FieldLabel htmlFor={`${id}-agent`} className={LABEL}>
                Take the notes from the prompts of an agent
              </FieldLabel>
              {/* The select has no value of its own: it shows the agent of the last pick. */}
              <Select onValueChange={take}>
                <SelectTrigger id={`${id}-agent`} size="sm" className="w-full">
                  <SelectValue placeholder="Pick an agent" />
                </SelectTrigger>
                <SelectContent>
                  {props.agents.map((agent) => (
                    <SelectItem key={agent.id} value={agent.id}>
                      {agentName(agent)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          ) : null}
          <div className="flex flex-wrap gap-2 border-t border-(--line) pt-2">
            <Button
              variant="outline"
              size="sm"
              className="ml-auto"
              onClick={() => props.onCancel()}
            >
              Cancel
            </Button>
            <Button type="submit" size="sm">
              Execute
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
