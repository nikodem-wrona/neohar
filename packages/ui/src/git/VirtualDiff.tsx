// The diff of one file in the Git view. It can have 20,000 lines, so the box paints only the rows
// on screen and some more on each side: each row has the same height, and a spacer of the whole
// height keeps the scroll bar true. A row keeps its line on one line and the box scrolls to the
// side, so that every row has its height.

import type { FileDiff } from '@neohar/transport';
import { Fragment, memo, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { lineSegments, marker } from '../rich/lines';
import { type DiffRow, diffRows, visibleRows } from './model';

/** The height of a row in CSS pixels, as `.git-vrow` sets it. */
const ROW = 18;
/** The rows that the box paints above and below the screen. */
const EXTRA = 40;

/** One row of the diff. A row that stays on screen while the box scrolls does not draw again. */
const VirtualRow = memo(function VirtualRow(props: { row: DiffRow }) {
  const row = props.row;
  if (row.kind === 'hunk') return <div className="diff-hunk-header git-vrow">{row.header}</div>;
  const line = row.line;
  return (
    <div className={`diff-line git-vrow diff-${line.kind}`}>
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
        {lineSegments(line).map((run, at) =>
          run.className ? (
            <span key={at} className={run.className}>
              {run.text}
            </span>
          ) : (
            <Fragment key={at}>{run.text}</Fragment>
          ),
        )}
      </span>
    </div>
  );
});

export function VirtualDiff(props: { file: FileDiff; label: string }) {
  const box = useRef<HTMLDivElement>(null);
  const rows = useMemo(() => diffRows(props.file), [props.file]);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(0);
  const range = visibleRows(rows.length, top, height, ROW, EXTRA);

  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight));
    observer.observe(element);
    setHeight(element.clientHeight);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      className="git-diff-body diff"
      role="group"
      aria-label={props.label}
      tabIndex={0}
      ref={box}
      onScroll={(event) => setTop(event.currentTarget.scrollTop)}
    >
      <div className="git-vspace" style={{ height: `${rows.length * ROW}px` }}>
        <div className="git-vrows" style={{ transform: `translateY(${range.start * ROW}px)` }}>
          {rows.slice(range.start, range.end).map((row, at) => (
            // The place of the row in the file: a row keeps its element while the box scrolls.
            <VirtualRow key={range.start + at} row={row} />
          ))}
        </div>
      </div>
    </div>
  );
}
