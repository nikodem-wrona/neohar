// The question before a pane, a tab or a workspace closes with programs in it. "Close" calls
// again with `force`, and the core kills every process of the panes. "Cancel", Escape or a
// click beside the box keeps everything. A failure of the call shows in the box. It is the Alert
// Dialog of shadcn/ui: the rest of the window is inert while it is open, and when it closes the
// focus goes back to where it was.

import { useRef } from 'react';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from '../components/ui/alert-dialog';
import { Button } from '../components/ui/button';
import { requestClose, type Closing } from './close';
import { useFocusReturn } from './focus';

export function CloseConfirm(props: { closing: Closing }) {
  const { transport, view, report } = useApp();
  const confirm = useRef<HTMLButtonElement>(null);
  const busy = useRef(false);
  const cancel = (): void => {
    view.closing.set(null);
  };
  const close = (): void => {
    if (busy.current) return;
    busy.current = true;
    requestClose(transport, props.closing, true)
      .then((next) => {
        busy.current = false;
        view.closing.set(next);
      })
      .catch(report);
  };
  // The question shows while the view has a close that waits.
  useLayer(true);
  // The focus goes back to the element that had it before the question, also when the pane took
  // it while the question closed.
  const focusBack = useFocusReturn(true, true);
  const what = props.closing.kind;
  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open) cancel();
      }}
    >
      {/* A click beside the box is a Cancel, as in the dialog before. The question asks before
          programs stop, so its border is red. */}
      <AlertDialogContent
        onOverlayClick={cancel}
        className="top-[18vh] block w-[min(56ch,calc(100vw-32px))] max-w-none translate-y-0 rounded-(--r-lg) border-destructive p-4 text-foreground data-[size=default]:sm:max-w-none"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          confirm.current?.focus();
        }}
        onCloseAutoFocus={focusBack}
      >
        <AlertDialogTitle className="mb-2 font-semibold [overflow-wrap:anywhere]">
          Close the {what} “{props.closing.name}”?
        </AlertDialogTitle>
        {props.closing.failure ? (
          <AlertDialogDescription className="mb-3 text-base text-destructive" role="alert">
            {props.closing.failure}
          </AlertDialogDescription>
        ) : (
          <AlertDialogDescription className="mb-3 text-base text-muted-foreground">
            {props.closing.kind !== 'workspace'
              ? 'Every process that runs in it stops.'
              : 'Its tabs close, and every process that runs in them stops.'}
            {props.closing.agent ? ' An agent runs in it, and its run stops too.' : null}
          </AlertDialogDescription>
        )}
        <div className="flex justify-end gap-2">
          <AlertDialogCancel size="sm" data-action="close-cancel">
            Cancel
          </AlertDialogCancel>
          {/* Dark text on the red: white text on it has too little contrast. */}
          <Button
            ref={confirm}
            size="sm"
            variant="destructive"
            className="bg-destructive font-semibold text-on-signal hover:bg-(--danger-hover) dark:bg-destructive"
            data-action="close-confirm"
            onClick={close}
          >
            Close {what}
          </Button>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  );
}
