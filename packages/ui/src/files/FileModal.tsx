// The file of the Files tab in a large view over the window. Open in the head of the file view, or
// in the menu of a file, shows it. It draws the same preview as the view under the tree
// (`fs.read`), with room to read it, and it has the actions of the file. Open in app gives the file
// to the editor of `ui.editor` or to the default app of the file. Escape, the close button or a
// click beside the box closes the view. It is a modal `<dialog>`: the rest of the window is inert
// while it is open, and when it closes the focus goes back to where it was.

import type { FsReadResult } from '@neohar/transport';
import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
import { outside } from '../shell/modal';
import { FileBody } from './FileBody';
import { FileIcon } from './FileIcon';
import type { MenuItem } from './model';

/** The buttons of the head. Open in app and Reveal act on the screen of the Mac. */
const ACTIONS: readonly (MenuItem & { short: string })[] = [
  { id: 'open-app', label: 'Open in app', short: 'Open in app' },
  { id: 'reveal', label: 'Reveal in Finder', short: 'Reveal' },
  { id: 'copy', label: 'Copy path', short: 'Copy' },
  { id: 'copy-relative', label: 'Copy relative path', short: 'Copy relative' },
  { id: 'insert', label: 'Insert into pane', short: 'Insert' },
];

export function FileModal(props: {
  /** The path of the open file. */
  path: string;
  preview: FsReadResult | null;
  error: string | null;
  /** The answer to the last action, which the tab also shows. */
  message: { text: string; error: boolean } | null;
  onAction: (id: MenuItem['id'], path: string) => void;
  onClose: () => void;
}) {
  const { form } = useApp();
  const actions = ACTIONS.filter(
    (action) => form !== 'phone' || (action.id !== 'open-app' && action.id !== 'reveal'),
  );
  // A Markdown file shows its HTML first. The switch shows its lines.
  const [source, setSource] = createSignal(false);
  createEffect(
    on(
      () => props.path,
      () => setSource(false),
      { defer: true },
    ),
  );

  let dialog: HTMLDialogElement | undefined;
  // The text takes the focus, so that the arrow keys and Page Down move it at once. While the file
  // is not read yet, the box holds the focus, and the text takes it when it comes.
  const focusText = (): void => {
    (dialog?.querySelector<HTMLElement>('.file-body') ?? dialog)?.focus({ preventScroll: true });
  };
  onMount(() => {
    dialog?.showModal();
    focusText();
  });
  createEffect(
    on(
      () => props.preview,
      () => {
        if (document.activeElement === dialog) focusText();
      },
      { defer: true },
    ),
  );
  // `close()` gives the focus back to the element that had it before the view.
  onCleanup(() => {
    if (dialog?.open) dialog.close();
  });

  return (
    // WebKit gives no focus to a button that the person clicks. The box takes it, so that Escape
    // still reaches it after a click on a button.
    <dialog
      class="file-modal"
      aria-labelledby="file-modal-title"
      tabIndex={-1}
      ref={(element) => (dialog = element)}
      onCancel={(event) => {
        event.preventDefault();
        props.onClose();
      }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && outside(event, event.currentTarget)) {
          props.onClose();
        }
      }}
    >
      <header class="file-modal-head">
        <Show when={props.preview}>{(preview) => <FileIcon icon={preview().icon} />}</Show>
        <h2 id="file-modal-title" class="file-modal-title">
          {props.preview?.name ?? props.path}
        </h2>
        <span class="file-modal-path" title={props.path}>
          {props.path}
        </span>
        <Show when={props.preview}>
          {(preview) => <span class="file-info">{preview().info}</span>}
        </Show>
        <button
          type="button"
          class="file-close"
          aria-label="Close the large view"
          title="Close (Escape)"
          onClick={() => props.onClose()}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </header>
      <div class="file-actions file-modal-actions" role="toolbar" aria-label="File actions">
        <For each={actions}>
          {(action) => (
            <button
              type="button"
              class="file-action"
              data-action={action.id}
              title={
                action.id === 'open-app'
                  ? 'Open the file with the editor of ui.editor, or with the default app of the file'
                  : action.label
              }
              aria-label={action.label}
              onClick={() => props.onAction(action.id, props.path)}
            >
              {action.short}
            </button>
          )}
        </For>
        <Show when={props.preview?.markdown}>
          <button
            type="button"
            class="file-action file-source"
            aria-pressed={source()}
            title={source() ? 'Show the Markdown' : 'Show the lines'}
            onClick={() => setSource(!source())}
          >
            {source() ? 'Rendered' : 'Source'}
          </button>
        </Show>
        <Show when={props.message}>
          {(note) => (
            <span
              class="files-message"
              classList={{ error: note().error }}
              role={note().error ? 'alert' : 'status'}
            >
              {note().text}
            </span>
          )}
        </Show>
      </div>
      <Show when={props.error}>
        <p class="file-error" role="alert">
          {props.error}
        </p>
      </Show>
      <Show
        when={props.preview}
        fallback={
          <Show when={!props.error}>
            <p class="drawer-empty">Reading the file…</p>
          </Show>
        }
      >
        {(preview) => <FileBody preview={preview()} source={source()} />}
      </Show>
    </dialog>
  );
}
