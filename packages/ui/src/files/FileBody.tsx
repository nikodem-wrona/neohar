// The text of a file of the Files tab, from `fs.read`: the view under the tree and the large view
// both draw it. The core sends one highlighted line for each line of the file, the HTML of a
// Markdown file, or an image as a `data:` URL. The body draws them and counts nothing: the number
// of a line is its place in the list.

import type { FsReadResult } from '@neohar/transport';
import { For, Show } from 'solid-js';
import { RichHtml } from '../rich/RichHtml';
import { segments } from '../rich/segments';

export function FileBody(props: {
  preview: FsReadResult;
  /** Show the lines of a Markdown file, and not its HTML. */
  source: boolean;
}) {
  const markdown = () => (props.source ? undefined : (props.preview.markdown ?? undefined));
  return (
    <div class="file-body" tabIndex={0} aria-label={`The text of ${props.preview.name}`}>
      <Show when={props.preview.kind === 'image' && props.preview.image}>
        {(image) => <img class="file-image" src={image()} alt={props.preview.name} />}
      </Show>
      <Show when={props.preview.kind === 'binary'}>
        <p class="drawer-empty">This file has no text to show.</p>
      </Show>
      <Show when={props.preview.kind === 'text'}>
        <Show
          when={markdown()}
          fallback={
            <Show
              when={props.preview.lines.length > 0}
              fallback={<p class="drawer-empty">The file is empty.</p>}
            >
              <div class="file-lines">
                <For each={props.preview.lines}>
                  {(line, index) => (
                    <div class="file-line">
                      <span class="file-num" aria-hidden="true">
                        {index() + 1}
                      </span>
                      <span class="file-text">
                        <For each={segments(line)}>
                          {(run) =>
                            run.className ? (
                              <span class={`hl-${run.className}`}>{run.text}</span>
                            ) : (
                              run.text
                            )
                          }
                        </For>
                      </span>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          }
        >
          {(html) => <RichHtml html={html().html} />}
        </Show>
      </Show>
      <Show when={props.preview.truncated}>
        <p class="file-truncated">The view shows only the start of this file.</p>
      </Show>
    </div>
  );
}
