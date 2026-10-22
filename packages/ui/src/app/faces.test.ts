import { expect, test } from 'bun:test';
import { fontStrings, within } from './faces';

test('the page loads the four variants of the family', () => {
  expect(fontStrings('"Hack Nerd Font Mono", Menlo', 26)).toEqual([
    '26px "Hack Nerd Font Mono", Menlo',
    'bold 26px "Hack Nerd Font Mono", Menlo',
    'italic 26px "Hack Nerd Font Mono", Menlo',
    'italic bold 26px "Hack Nerd Font Mono", Menlo',
  ]);
});

test('a wait ends with the value, with nothing after the time, and with nothing on an error', async () => {
  expect(await within(Promise.resolve(3), 50)).toBe(3);
  expect(await within(new Promise<number>(() => undefined), 10)).toBeUndefined();
  expect(await within(Promise.reject(new Error('no')), 50)).toBeUndefined();
});
