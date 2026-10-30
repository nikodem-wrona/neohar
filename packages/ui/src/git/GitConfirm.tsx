// The question of the core before a write of the Git view: an agent works in the worktree, the
// worktree has changes to bring or to leave, or another worktree has the branch out. The core
// writes the text; this dialog gives the answers that its kind has. Cancel, Escape or a click
// beside the box keeps everything. It is a modal `<dialog>`, as the close confirmation is.

import { type MouseEvent, useLayoutEffect, useRef } from 'react';
import { useLatest } from '../lib/hooks';
import { cn } from '../lib/utils';
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
  const dialog = useRef<HTMLDialogElement>(null);
  const first = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const kind = useLatest(props.question.kind);
  // Under a working agent, Enter must not change its files: the focus starts on Cancel.
  // A layout effect: its cleanup runs before the dialog leaves the page, so `close()` can give
  // the focus back.
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    (kind.current === 'agents' ? cancel : first).current?.focus();
    return () => {
      if (element?.open) element.close();
    };
  }, [kind]);
  return (
    <dialog
      className={cn('confirm git-confirm', {
        'git-confirm-danger': props.question.kind === 'agents',
      })}
      role="alertdialog"
      aria-labelledby="git-confirm-title"
      aria-describedby="git-confirm-text"
      ref={dialog}
      onCancel={(event) => {
        event.preventDefault();
        props.onCancel();
      }}
      onMouseDown={(event: MouseEvent<HTMLDialogElement>) => {
        if (
          event.target === event.currentTarget &&
          outside(event.nativeEvent, event.currentTarget)
        ) {
          props.onCancel();
        }
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <div className="confirm-box">
        <h2 id="git-confirm-title" className="confirm-title">
          {titleOf(props.question)}
        </h2>
        <p id="git-confirm-text" className="confirm-text">
          {props.question.text}
        </p>
        <div className="confirm-buttons">
          <button
            type="button"
            className="confirm-cancel"
            ref={cancel}
            onClick={() => props.onCancel()}
          >
            Cancel
          </button>
          {props.answers.map((answer, index) => (
            <button
              key={answer.label}
              type="button"
              className={cn({ 'confirm-danger': answer.danger === true }) || undefined}
              ref={index === 0 ? first : undefined}
              onClick={() => answer.run()}
            >
              {answer.label}
            </button>
          ))}
        </div>
      </div>
    </dialog>
  );
}
