import { expect, test } from 'bun:test';
import { cn } from './utils';

test('the class list has the names that are on, in their order', () => {
  expect(cn('tab', false, null, undefined, { active: true, lifted: false }, 'end')).toBe(
    'tab active end',
  );
  expect(cn()).toBe('');
});
