import { expect, test } from 'bun:test';
import { segments } from './highlight';

test('a title without ranges is one segment', () => {
  expect(segments('New tab', [])).toEqual([{ text: 'New tab', hit: false }]);
  expect(segments('', [])).toEqual([]);
});

test('the ranges cut the title into hits and the rest', () => {
  expect(
    segments('New tab', [
      [0, 3],
      [4, 7],
    ]),
  ).toEqual([
    { text: 'New', hit: true },
    { text: ' ', hit: false },
    { text: 'tab', hit: true },
  ]);
  expect(segments('Split pane', [[0, 1]])).toEqual([
    { text: 'S', hit: true },
    { text: 'plit pane', hit: false },
  ]);
});

test('a range outside the title is cut, and an overlap does not repeat text', () => {
  expect(segments('abc', [[1, 9]])).toEqual([
    { text: 'a', hit: false },
    { text: 'bc', hit: true },
  ]);
  expect(
    segments('abcd', [
      [0, 3],
      [2, 4],
    ]),
  ).toEqual([
    { text: 'abc', hit: true },
    { text: 'd', hit: true },
  ]);
});

test('the offsets count UTF-16 code units, so an emoji is two', () => {
  expect(segments('🙂 tab', [[3, 6]])).toEqual([
    { text: '🙂 ', hit: false },
    { text: 'tab', hit: true },
  ]);
});
