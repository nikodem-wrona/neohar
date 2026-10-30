// The question of the core before a write of the Git view: an agent works in the worktree, the
// worktree has changes to bring or to leave, or another worktree has the branch out. The core
// writes the text; this dialog gives the answers that its kind has. Cancel, Escape or a click
// beside the box keeps everything. It is the Alert Dialog of shadcn/ui: the rest of the window is
// inert while it is open, and when it closes the focus goes back to where it was.

import { useRef } from 'react';
import { useLayer } from '../app/layer';
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from '../components/ui/alert-dialog';
import { Button } from '../components/ui/button';
import { cn } from '../lib/utils';
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
  const first = useRef<HTMLButtonElement>(null);
  const danger = props.question.kind === 'agents';
  // The dialog shows while the view has a question.
  useLayer(true);
  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open) props.onCancel();
      }}
    >
      <AlertDialogContent
        // A click beside the box is a Cancel, as in the dialog before.
        onOverlayClick={() => props.onCancel()}
        className={cn(
          'top-[18vh] block w-[min(56ch,calc(100vw-32px))] max-w-none translate-y-0 rounded-(--r-lg) p-4 text-foreground data-[size=default]:sm:max-w-none',
          danger ? 'border-destructive' : 'border-border',
        )}
        // Under a working agent, Enter must not change its files: the focus starts on Cancel,
        // as Radix does. Else it starts on the first answer.
        onOpenAutoFocus={(event) => {
          if (danger) return;
          event.preventDefault();
          first.current?.focus();
        }}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <AlertDialogTitle className="mb-2 [font:var(--t-title)] font-semibold [overflow-wrap:anywhere]">
          {titleOf(props.question)}
        </AlertDialogTitle>
        <AlertDialogDescription className="mb-3 text-[13px]/[18px] text-muted-foreground">
          {props.question.text}
        </AlertDialogDescription>
        <div className="flex justify-end gap-2">
          <AlertDialogCancel size="sm">Cancel</AlertDialogCancel>
          {props.answers.map((answer, index) => (
            <Button
              key={answer.label}
              ref={index === 0 ? first : undefined}
              size="sm"
              variant={answer.danger === true ? 'destructive' : 'outline'}
              className={cn(
                answer.danger === true &&
                  'bg-destructive font-semibold text-on-signal hover:bg-(--danger-hover) dark:bg-destructive',
              )}
              onClick={() => answer.run()}
            >
              {answer.label}
            </Button>
          ))}
        </div>
      </AlertDialogContent>
    </AlertDialog>
  );
}
