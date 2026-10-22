// The question before a pane, a tab or a workspace closes with programs in it. "Close" calls
// again with `force`, and the core kills every process of the panes. "Cancel", Escape or a
// click beside the box keeps everything. A failure of the call shows in the box. It is a modal
// `<dialog>`: the rest of the window is inert while it is open, and when it closes the focus goes
// back to where it was.

import { onCleanup, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
import { outside } from './modal';
import { requestClose, type Closing } from './close';

export function CloseConfirm(props: { closing: Closing }) {
  const { transport, view, report } = useApp();
  let confirm: HTMLButtonElement | undefined;
  let busy = false;
  const cancel = (): void => {
    view.setClosing(null);
  };
  const close = (): void => {
    if (busy) return;
    busy = true;
    requestClose(transport, props.closing, true)
      .then((next) => {
        busy = false;
        view.setClosing(next);
      })
      .catch(report);
  };
  let dialog: HTMLDialogElement | undefined;
  onMount(() => {
    dialog?.showModal();
    confirm?.focus();
  });
  // `close()` gives the focus back to the element that had it before the question.
  onCleanup(() => {
    if (dialog?.open) dialog.close();
  });
  const what = () => props.closing.kind;
  return (
    <dialog
      class="confirm"
      role="alertdialog"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-text"
      ref={(element) => (dialog = element)}
      onCancel={(e) => {
        e.preventDefault();
        cancel();
      }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && outside(e, e.currentTarget)) cancel();
      }}
    >
      <div class="confirm-box">
        <h2 id="confirm-title" class="confirm-title">
          Close the {what()} “{props.closing.name}”?
        </h2>
        <Show
          when={props.closing.failure}
          fallback={
            <p id="confirm-text" class="confirm-text">
              <Show
                when={props.closing.kind !== 'workspace'}
                fallback="Its tabs close, and every process that runs in them stops."
              >
                Every process that runs in it stops.
              </Show>
              <Show when={props.closing.agent}> An agent runs in it, and its run stops too.</Show>
            </p>
          }
        >
          {(failure) => (
            <p id="confirm-text" class="confirm-text confirm-failure" role="alert">
              {failure()}
            </p>
          )}
        </Show>
        <div class="confirm-buttons">
          <button type="button" class="confirm-cancel" data-action="close-cancel" onClick={cancel}>
            Cancel
          </button>
          <button
            type="button"
            class="confirm-danger"
            data-action="close-confirm"
            ref={(element) => (confirm = element)}
            onClick={close}
          >
            Close {what()}
          </button>
        </div>
      </div>
    </dialog>
  );
}
