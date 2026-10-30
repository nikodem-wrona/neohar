// The question before a pane, a tab or a workspace closes with programs in it. "Close" calls
// again with `force`, and the core kills every process of the panes. "Cancel", Escape or a
// click beside the box keeps everything. A failure of the call shows in the box. It is a modal
// `<dialog>`: the rest of the window is inert while it is open, and when it closes the focus goes
// back to where it was.

import { useLayoutEffect, useRef } from 'react';
import { useApp } from '../app/context';
import { outside } from './modal';
import { requestClose, type Closing } from './close';

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
  const dialog = useRef<HTMLDialogElement>(null);
  // A layout effect: its cleanup runs before the dialog leaves the page, so `close()` can give
  // the focus back.
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    confirm.current?.focus();
    // `close()` gives the focus back to the element that had it before the question.
    return () => {
      if (element?.open) element.close();
    };
  }, []);
  const what = props.closing.kind;
  return (
    <dialog
      className="confirm"
      role="alertdialog"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-text"
      ref={dialog}
      onCancel={(e) => {
        e.preventDefault();
        cancel();
      }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && outside(e.nativeEvent, e.currentTarget)) cancel();
      }}
    >
      <div className="confirm-box">
        <h2 id="confirm-title" className="confirm-title">
          Close the {what} “{props.closing.name}”?
        </h2>
        {props.closing.failure ? (
          <p id="confirm-text" className="confirm-text confirm-failure" role="alert">
            {props.closing.failure}
          </p>
        ) : (
          <p id="confirm-text" className="confirm-text">
            {props.closing.kind !== 'workspace'
              ? 'Every process that runs in it stops.'
              : 'Its tabs close, and every process that runs in them stops.'}
            {props.closing.agent ? ' An agent runs in it, and its run stops too.' : null}
          </p>
        )}
        <div className="confirm-buttons">
          <button
            type="button"
            className="confirm-cancel"
            data-action="close-cancel"
            onClick={cancel}
          >
            Cancel
          </button>
          <button
            type="button"
            className="confirm-danger"
            data-action="close-confirm"
            ref={confirm}
            onClick={close}
          >
            Close {what}
          </button>
        </div>
      </div>
    </dialog>
  );
}
