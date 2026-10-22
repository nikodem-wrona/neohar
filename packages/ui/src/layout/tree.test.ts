import { expect, test } from 'bun:test';
import type { LayoutNode } from '@neohar/transport';
import { asSplit, flexOf, paneIds } from './tree';

const tree: LayoutNode = {
  type: 'split',
  axis: 'row',
  ratios: [0.5, 0.5],
  children: [
    { type: 'pane', pane: 'A' },
    {
      type: 'split',
      axis: 'column',
      ratios: [0.25, 0.75],
      children: [
        { type: 'pane', pane: 'B' },
        { type: 'pane', pane: 'C' },
      ],
    },
  ],
};

test('the panes of a tree in the order of the layout', () => {
  expect(paneIds(tree)).toEqual(['A', 'B', 'C']);
  expect(paneIds({ type: 'pane', pane: 'Z' })).toEqual(['Z']);
});

test('a split is a split and a pane is not', () => {
  expect(asSplit(tree)?.axis).toBe('row');
  expect(asSplit({ type: 'pane', pane: 'A' })).toBeUndefined();
});

test('the share of a child is its grow factor', () => {
  expect(flexOf(0.25)).toBe('0.25 1 0%');
  expect(flexOf(undefined)).toBe('1 1 0%');
  expect(flexOf(0)).toBe('1 1 0%');
});
