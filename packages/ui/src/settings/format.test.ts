import { expect, test } from 'bun:test';
import { fieldText, humanize, parseEdit } from './format';

test('a name reads as a label', () => {
  expect(humanize('font_size')).toBe('Font size');
  expect(humanize('ui')).toBe('Ui');
  expect(humanize('macos_option_as_alt')).toBe('Macos option as alt');
});

test('a number field needs a number', () => {
  expect(parseEdit('15', 13)).toEqual({ ok: true, value: 15 });
  expect(parseEdit(' 2.5 ', 1)).toEqual({ ok: true, value: 2.5 });
  expect(parseEdit('', 13).ok).toBe(false);
  expect(parseEdit('big', 13).ok).toBe(false);
});

test('a list or a table needs JSON', () => {
  expect(parseEdit('[1, 2]', [41000, 41999])).toEqual({ ok: true, value: [1, 2] });
  expect(parseEdit('{"a": 1}', {})).toEqual({ ok: true, value: { a: 1 } });
  expect(parseEdit('[1,', [1]).ok).toBe(false);
});

test('a string is the text', () => {
  expect(parseEdit('mod+j', 'mod+k')).toEqual({ ok: true, value: 'mod+j' });
  expect(parseEdit('', 'x')).toEqual({ ok: true, value: '' });
});

test('a field shows a string as it is and anything else as JSON', () => {
  expect(fieldText('Menlo')).toBe('Menlo');
  expect(fieldText(13)).toBe('13');
  expect(fieldText([1, 2])).toBe('[1,2]');
  expect(fieldText(true)).toBe('true');
});
