// The file of the Files tab in a large view over the window. Open in the head of the file view, or
// in the menu of a file, shows it. It draws the same preview as the view under the tree
// (`fs.read`), with room to read it, and it has the actions of the file. Open in app gives the file
// to the editor of `ui.editor` or to the default app of the file. Escape, the close button or a
// click beside the box closes the view. It is a modal `<dialog>`: the rest of the window is inert
// while it is open, and when it closes the focus goes back to where it was.

import type { FsReadResult } from '@neohar/transport';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { cn } from '../lib/utils';
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

/** The text takes the focus, else the box. */
function focusText(dialog: HTMLDialogElement | null): void {
  (dialog?.querySelector<HTMLElement>('.file-body') ?? dialog)?.focus({ preventScroll: true });
}

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
  const [source, setSource] = useState(false);
  // Another file shows its HTML first again.
  const [sourceOf, setSourceOf] = useState(props.path);
  if (sourceOf !== props.path) {
    setSourceOf(props.path);
    setSource(false);
  }

  const dialog = useRef<HTMLDialogElement>(null);
  // The text takes the focus, so that the arrow keys and Page Down move it at once. While the file
  // is not read yet, the box holds the focus, and the text takes it when it comes.
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    focusText(element);
    // `close()` gives the focus back to the element that had it before the view.
    return () => {
      if (element?.open) element.close();
    };
  }, []);
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    if (document.activeElement === dialog.current) focusText(dialog.current);
  }, [props.preview]);

  return (
    // WebKit gives no focus to a button that the person clicks. The box takes it, so that Escape
    // still reaches it after a click on a button.
    <dialog
      className="file-modal"
      aria-labelledby="file-modal-title"
      tabIndex={-1}
      ref={dialog}
      onCancel={(event) => {
        event.preventDefault();
        props.onClose();
      }}
      onMouseDown={(event) => {
        if (
          event.target === event.currentTarget &&
          outside(event.nativeEvent, event.currentTarget)
        ) {
          props.onClose();
        }
      }}
    >
      <header className="file-modal-head">
        {props.preview ? <FileIcon icon={props.preview.icon} /> : null}
        <h2 id="file-modal-title" className="file-modal-title">
          {props.preview?.name ?? props.path}
        </h2>
        <span className="file-modal-path" title={props.path}>
          {props.path}
        </span>
        {props.preview ? <span className="file-info">{props.preview.info}</span> : null}
        <button
          type="button"
          className="file-close"
          aria-label="Close the large view"
          title="Close (Escape)"
          onClick={() => props.onClose()}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </button>
      </header>
      <div className="file-actions file-modal-actions" role="toolbar" aria-label="File actions">
        {actions.map((action) => (
          <button
            key={action.id}
            type="button"
            className="file-action"
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
        ))}
        {props.preview?.markdown ? (
          <button
            type="button"
            className="file-action file-source"
            aria-pressed={source}
            title={source ? 'Show the Markdown' : 'Show the lines'}
            onClick={() => setSource(!source)}
          >
            {source ? 'Rendered' : 'Source'}
          </button>
        ) : null}
        {props.message ? (
          <span
            className={cn('files-message', { error: props.message.error })}
            role={props.message.error ? 'alert' : 'status'}
          >
            {props.message.text}
          </span>
        ) : null}
      </div>
      {props.error ? (
        <p className="file-error" role="alert">
          {props.error}
        </p>
      ) : null}
      {props.preview ? (
        <FileBody preview={props.preview} source={source} />
      ) : !props.error ? (
        <p className="drawer-empty">Reading the file…</p>
      ) : null}
    </dialog>
  );
}
