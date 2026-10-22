import { describe, expect, it } from 'bun:test';
import { segments } from './segments';

describe('segments', () => {
  it('cuts the text at the ranges and keeps the gaps', () => {
    expect(
      segments({
        text: 'git commit -m "x"',
        spans: [
          [0, 3, 'command'],
          [4, 10, 'arg'],
          [14, 17, 'string'],
        ],
      }),
    ).toEqual([
      { text: 'git', className: 'command' },
      { text: ' ', className: null },
      { text: 'commit', className: 'arg' },
      { text: ' -m ', className: null },
      { text: '"x"', className: 'string' },
    ]);
  });

  it('counts in UTF-16 code units, as the core does', () => {
    // The emoji is two code units. The range after it starts at 3.
    const found = segments({ text: '😀 ls', spans: [[3, 5, 'command']] });
    expect(found).toEqual([
      { text: '😀 ', className: null },
      { text: 'ls', className: 'command' },
    ]);
  });

  it('puts the whole text in one run when there are no ranges', () => {
    expect(segments({ text: 'plain', spans: [] })).toEqual([{ text: 'plain', className: null }]);
    expect(segments({ text: '', spans: [] })).toEqual([]);
  });

  it('clamps the ranges that overlap or run past the end', () => {
    expect(
      segments({
        text: 'abcdef',
        spans: [
          [0, 4, 'head'],
          [3, 99, 'tail'],
        ],
      }),
    ).toEqual([
      { text: 'abcd', className: 'head' },
      { text: 'ef', className: 'tail' },
    ]);
  });

  it('drops a range that is empty or lies outside the text', () => {
    expect(
      segments({
        text: 'abc',
        spans: [
          [2, 2, 'empty'],
          [10, 12, 'outside'],
        ],
      }),
    ).toEqual([{ text: 'abc', className: null }]);
  });
});
