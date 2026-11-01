import { expect, test } from 'bun:test';
import { cn } from './utils';

test('the class list has the names that are on, in their order', () => {
  expect(cn('tab', false, null, undefined, { active: true, lifted: false }, 'end')).toBe(
    'tab active end',
  );
  expect(cn()).toBe('');
});

test('of two Tailwind classes for the same property, the last one stays', () => {
  expect(cn('px-2 py-1', 'px-4')).toBe('py-1 px-4');
  expect(cn('pane focused', 'rounded-md')).toBe('pane focused rounded-md');
});
