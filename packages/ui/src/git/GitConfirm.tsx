// The question of the core before a write of the Git view: an agent works in the worktree, the
// worktree has changes to bring or to leave, or another worktree has the branch out. The core
// writes the text; this dialog gives the answers that its kind has. Cancel, Escape or a click
// beside the box keeps everything. It is a modal `<dialog>`, as the close confirmation is.

import { For, onCleanup, onMount } from 'solid-js';
import { outside } from '../shell/modal';
import type { GitQuestion } from './model';

export interface GitAnswer {
  label: string;
  run: () => void;
  /** The answer changes the files under a working agent. */
  danger?: boolean;
}

/** The title of a question. */
function titleOf(question: GitQuestion): string {
  switch (question.kind) {
    case 'agents':
      return 'An agent works in this worktree';
    case 'changes':
      return 'The worktree has changes';
    default:
      return 'Another worktree has this branch';
  }
}

export function GitConfirm(props: {
  question: GitQuestion;
  answers: GitAnswer[];
  onCancel: () => void;
}) {
  let dialog: HTMLDialogElement | undefined;
  let first: HTMLButtonElement | undefined;
  let cancel: HTMLButtonElement | undefined;
  // Under a working agent, Enter must not change its files: the focus starts on Cancel.
  onMount(() => {
    dialog?.showModal();
    (props.question.kind === 'agents' ? cancel : first)?.focus();
  });
  onCleanup(() => {
    if (dialog?.open) dialog.close();
  });
  return (
    <dialog
      class="confirm git-confirm"
      classList={{ 'git-confirm-danger': props.question.kind === 'agents' }}
      role="alertdialog"
      aria-labelledby="git-confirm-title"
      aria-describedby="git-confirm-text"
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
      <div class="confirm-box">
        <h2 id="git-confirm-title" class="confirm-title">
          {titleOf(props.question)}
        </h2>
        <p id="git-confirm-text" class="confirm-text">
          {props.question.text}
        </p>
        <div class="confirm-buttons">
          <button
            type="button"
            class="confirm-cancel"
            ref={(element) => (cancel = element)}
            onClick={() => props.onCancel()}
          >
            Cancel
          </button>
          <For each={props.answers}>
            {(answer, index) => (
              <button
                type="button"
                classList={{ 'confirm-danger': answer.danger === true }}
                ref={(element) => {
                  if (index() === 0) first = element;
                }}
                onClick={() => answer.run()}
              >
                {answer.label}
              </button>
            )}
          </For>
        </div>
      </div>
    </dialog>
  );
}
