// The diff of a card: files, hunks and lines, drawn from the data of the core. The
// text of a line is highlighted by ranges, and the characters that changed inside it are marked.

import type { Diff as DiffValue, FileDiff } from '@neohar/transport';
import { fileTitle, lineSegments, marker } from './lines';

/** The hunks of one file and the note of a cut, without the title of the file. */
export function DiffHunks(props: { file: FileDiff }) {
  // The hunks, the lines and the runs of a diff do not move: their place is their key.
  return (
    <>
      {props.file.hunks.map((hunk, h) => (
        <div key={h} className="diff-hunk">
          <div className="diff-hunk-header">{hunk.header}</div>
          {hunk.lines.map((line, l) => (
            <div key={l} className={`diff-line diff-${line.kind}`}>
              <span className="diff-num" aria-hidden="true">
                {line.old ?? ''}
              </span>
              <span className="diff-num" aria-hidden="true">
                {line.new ?? ''}
              </span>
              <span className="diff-marker" aria-hidden="true">
                {marker(line.kind)}
              </span>
              <span className="diff-text">
                {lineSegments(line).map((run, r) =>
                  run.className ? (
                    <span key={r} className={run.className}>
                      {run.text}
                    </span>
                  ) : (
                    run.text
                  ),
                )}
              </span>
            </div>
          ))}
        </div>
      ))}
      {props.file.truncated ? (
        <p className="diff-truncated">The diff of this file is cut at 2,000 lines.</p>
      ) : null}
    </>
  );
}

function File(props: { file: FileDiff }) {
  return (
    <details className="diff-file" open>
      <summary className="diff-title">{fileTitle(props.file)}</summary>
      <DiffHunks file={props.file} />
    </details>
  );
}

export function Diff(props: { value: DiffValue }) {
  return (
    <div className="diff" role="group" aria-label="Changes">
      {props.value.files.map((file, at) => (
        <File key={at} file={file} />
      ))}
    </div>
  );
}
