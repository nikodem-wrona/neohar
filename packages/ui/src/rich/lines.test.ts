import { describe, expect, it } from 'bun:test';
import type { DiffLine, FileDiff } from '@neohar/transport';
import { fileTitle, INTRA_CLASS, lineSegments, marker } from './lines';

const line = (
  text: string,
  spans: [number, number, string][],
  intra?: [number, number][],
): DiffLine => ({
  kind: 'add',
  old: null,
  new: 1,
  text: { text, spans },
  intra: intra ?? null,
});

describe('lineSegments', () => {
  it('is the spans of the text when nothing changed inside the line', () => {
    expect(lineSegments(line('let a = 1;', [[0, 3, 'keyword']]))).toEqual([
      { text: 'let', className: 'keyword' },
      { text: ' a = 1;', className: null },
    ]);
  });

  it('marks the changed range, also across a syntax range', () => {
    const found = lineSegments(
      line(
        'let value = 2;',
        [
          [0, 3, 'keyword'],
          [4, 9, 'name'],
        ],
        [[8, 13]],
      ),
    );
    expect(found).toEqual([
      { text: 'let', className: 'keyword' },
      { text: ' ', className: null },
      { text: 'valu', className: 'name' },
      { text: 'e', className: `name ${INTRA_CLASS}` },
      { text: ` = 2`.slice(0, 4), className: INTRA_CLASS },
      { text: ';', className: null },
    ]);
  });

  it('keeps all the text', () => {
    const l = line('abcdef', [[1, 3, 'x']], [[2, 5]]);
    expect(
      lineSegments(l)
        .map((s) => s.text)
        .join(''),
    ).toBe('abcdef');
  });
});

describe('marker and fileTitle', () => {
  it('shows the sign of a line', () => {
    expect(marker('add')).toBe('+');
    expect(marker('del')).toBe('−');
    expect(marker('ctx')).toBe(' ');
  });

  it('shows the path and the counts, and the status when the file is not only changed', () => {
    const file: FileDiff = {
      path: 'src/a.rs',
      status: 'modified',
      stats: { add: 3, del: 1 },
      hunks: [],
    };
    expect(fileTitle(file)).toBe('src/a.rs  +3 −1');
    expect(fileTitle({ ...file, status: 'added' })).toBe('added src/a.rs  +3 −1');
  });
});
