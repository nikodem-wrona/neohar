// The file that the person opened in the Files tab, from `fs.read`. The core
// sends one highlighted line for each line of the file, the HTML of a Markdown file, or an image
// as a `data:` URL, and the text of the head. Open shows the same file in a large view over the
// window (`FileModal.tsx`).

import type { FsReadResult } from '@neohar/transport';
import { createEffect, createSignal, For, on, Show } from 'solid-js';
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
  const [source, setSource] = createSignal(false);
  // Reveal acts on the screen of the Mac: a phone does not show it.
  const { form } = useApp();
  const actions = ACTIONS.filter((action) => form !== 'phone' || action.id !== 'reveal');
  createEffect(
    on(
      () => props.path,
      () => setSource(false),
    ),
  );

  return (
    <section class="file-view" aria-label="File">
      <header class="file-head">
        <Show when={props.preview}>{(preview) => <FileIcon icon={preview().icon} />}</Show>
        <span class="file-name" title={props.path}>
          {props.preview?.name ?? props.path}
        </span>
        <Show when={props.preview}>
          {(preview) => <span class="file-info">{preview().info}</span>}
        </Show>
        <span class="file-spacer" />
        <button
          type="button"
          class="file-close"
          aria-label="Close the file"
          title="Close the file"
          onClick={() => props.onClose()}
        >
          <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </header>
      <div class="file-actions" role="toolbar" aria-label="File actions">
        <For each={actions}>
          {(action) => (
            <button
              type="button"
              class="file-action"
              data-action={action.id}
              title={action.id === 'open' ? 'Open the file in a large view' : action.label}
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
      </div>
      <Show when={props.error}>
        <p class="file-error" role="alert">
          {props.error}
        </p>
      </Show>
      <Show when={props.preview}>
        {(preview) => <FileBody preview={preview()} source={source()} />}
      </Show>
    </section>
  );
}
