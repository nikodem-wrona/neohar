// The file that the person opened in the Files tab, from `fs.read`. The core
// sends one highlighted line for each line of the file, the HTML of a Markdown file, or an image
// as a `data:` URL, and the text of the head. Open shows the same file in a large view over the
// window (`FileModal.tsx`).

import type { FsReadResult } from '@neohar/transport';
import { useState } from 'react';
import { useApp } from '../app/context';
import { FileBody } from './FileBody';
import { FileIcon } from './FileIcon';
import type { MenuItem } from './model';

/** The buttons of the head, in the order of the menu of a row. */
const ACTIONS: readonly (MenuItem & { short: string })[] = [
  { id: 'open', label: 'Open', short: 'Open' },
  { id: 'reveal', label: 'Reveal in Finder', short: 'Reveal' },
  { id: 'copy', label: 'Copy path', short: 'Copy' },
  { id: 'copy-relative', label: 'Copy relative path', short: 'Copy relative' },
  { id: 'insert', label: 'Insert into pane', short: 'Insert' },
];

export function FileView(props: {
  /** The path of the open file. */
  path: string;
  preview: FsReadResult | null;
  error: string | null;
  onAction: (id: MenuItem['id'], path: string) => void;
  onClose: () => void;
}) {
  // A Markdown file shows its HTML first. The switch shows its lines.
  const [source, setSource] = useState(false);
  // Reveal acts on the screen of the Mac: a phone does not show it.
  const { form } = useApp();
  const actions = ACTIONS.filter((action) => form !== 'phone' || action.id !== 'reveal');
  // Another file shows its HTML first again.
  const [sourceOf, setSourceOf] = useState(props.path);
  if (sourceOf !== props.path) {
    setSourceOf(props.path);
    setSource(false);
  }

  return (
    <section className="file-view" aria-label="File">
      <header className="file-head">
        {props.preview ? <FileIcon icon={props.preview.icon} /> : null}
        <span className="file-name" title={props.path}>
          {props.preview?.name ?? props.path}
        </span>
        {props.preview ? <span className="file-info">{props.preview.info}</span> : null}
        <span className="file-spacer" />
        <button
          type="button"
          className="file-close"
          aria-label="Close the file"
          title="Close the file"
          onClick={() => props.onClose()}
        >
          <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </button>
      </header>
      <div className="file-actions" role="toolbar" aria-label="File actions">
        {actions.map((action) => (
          <button
            key={action.id}
            type="button"
            className="file-action"
            data-action={action.id}
            title={action.id === 'open' ? 'Open the file in a large view' : action.label}
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
      </div>
      {props.error ? (
        <p className="file-error" role="alert">
          {props.error}
        </p>
      ) : null}
      {props.preview ? <FileBody preview={props.preview} source={source} /> : null}
    </section>
  );
}
