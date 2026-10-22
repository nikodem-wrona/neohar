// The diff of a card: files, hunks and lines, drawn from the data of the core. The
// text of a line is highlighted by ranges, and the characters that changed inside it are marked.

import type { Diff as DiffValue, FileDiff } from '@neohar/transport';
import { For, Show } from 'solid-js';
import { fileTitle, lineSegments, marker } from './lines';

/** The hunks of one file and the note of a cut, without the title of the file. */
export function DiffHunks(props: { file: FileDiff }) {
  return (
    <>
      <For each={props.file.hunks}>
        {(hunk) => (
          <div class="diff-hunk">
            <div class="diff-hunk-header">{hunk.header}</div>
            <For each={hunk.lines}>
              {(line) => (
                <div class="diff-line" classList={{ [`diff-${line.kind}`]: true }}>
                  <span class="diff-num" aria-hidden="true">
                    {line.old ?? ''}
                  </span>
                  <span class="diff-num" aria-hidden="true">
                    {line.new ?? ''}
                  </span>
                  <span class="diff-marker" aria-hidden="true">
                    {marker(line.kind)}
                  </span>
                  <span class="diff-text">
                    <For each={lineSegments(line)}>
                      {(run) =>
                        run.className ? <span class={run.className}>{run.text}</span> : run.text
                      }
                    </For>
                  </span>
                </div>
              )}
            </For>
          </div>
        )}
      </For>
      <Show when={props.file.truncated}>
        <p class="diff-truncated">The diff of this file is cut at 2,000 lines.</p>
      </Show>
    </>
  );
}

function File(props: { file: FileDiff }) {
  return (
    <details class="diff-file" open>
      <summary class="diff-title">{fileTitle(props.file)}</summary>
      <DiffHunks file={props.file} />
    </details>
  );
}

export function Diff(props: { value: DiffValue }) {
  return (
    <div class="diff" role="group" aria-label="Changes">
      <For each={props.value.files}>{(file) => <File file={file} />}</For>
    </div>
  );
}
