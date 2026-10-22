import { expect, test } from 'bun:test';
import { isWindowId, parseParams } from './params';

const ID = '01JD3Q9X5K7M2N4P6R8T0V1W2Y';

test('a ULID in the query names the window', () => {
  expect(parseParams(`?window=${ID}`)).toEqual({ window: ID, view: 'main' });
  expect(parseParams(`?ws=4000&token=a&window=${ID}&view=settings`)).toEqual({
    window: ID,
    view: 'settings',
  });
});

test('a label that is not a window id and a missing window give none', () => {
  expect(parseParams('?window=main').window).toBeNull();
  expect(parseParams('').window).toBeNull();
  expect(parseParams(`?window=${ID.toLowerCase()}`).window).toBeNull();
  expect(parseParams(`?window=${ID}0`).window).toBeNull();
});

test('an unknown view is the main view', () => {
  expect(parseParams(`?window=${ID}&view=other`).view).toBe('main');
  expect(isWindowId(null)).toBe(false);
});

test('the settings window of a machine names the machine', () => {
  expect(parseParams('?view=settings&machine=studio').machine).toBe('studio');
  expect(parseParams('?view=settings').machine).toBeUndefined();
  expect(parseParams('?view=settings&machine=Bad/Label').machine).toBeUndefined();
});
