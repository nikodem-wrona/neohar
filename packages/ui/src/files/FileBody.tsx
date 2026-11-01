// The text of a file of the Files tab, from `fs.read`: the view under the tree and the large view
// both draw it. The core sends one highlighted line for each line of the file, the HTML of a
// Markdown file, or an image as a `data:` URL. The body draws them and counts nothing: the number
// of a line is its place in the list.

import type { FsReadResult } from '@neohar/transport';
import { RichHtml } from '../rich/RichHtml';
import { segments } from '../rich/segments';

export function FileBody(props: {
  preview: FsReadResult;
  /** Show the lines of a Markdown file, and not its HTML. */
  source: boolean;
}) {
  const markdown = props.source ? undefined : (props.preview.markdown ?? undefined);
  const image = props.preview.kind === 'image' && props.preview.image;
  return (
    <div className="file-body" tabIndex={0} aria-label={`The text of ${props.preview.name}`}>
      {image ? <img className="file-image" src={image} alt={props.preview.name} /> : null}
      {props.preview.kind === 'binary' ? (
        <p className="drawer-empty">This file has no text to show.</p>
      ) : null}
      {props.preview.kind === 'text' ? (
        markdown ? (
          <RichHtml html={markdown.html} />
        ) : props.preview.lines.length > 0 ? (
          <div className="file-lines">
            {/* The lines and the runs of a file do not move: their place is their key. */}
            {props.preview.lines.map((line, index) => (
              <div key={index} className="file-line">
                <span className="file-num" aria-hidden="true">
                  {index + 1}
                </span>
                <span className="file-text">
                  {segments(line).map((run, at) =>
                    run.className ? (
                      <span key={at} className={`hl-${run.className}`}>
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
        ) : (
          <p className="drawer-empty">The file is empty.</p>
        )
      ) : null}
      {props.preview.truncated ? (
        <p className="file-truncated">The view shows only the start of this file.</p>
      ) : null}
    </div>
  );
}
