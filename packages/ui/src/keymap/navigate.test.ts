import { expect, test } from 'bun:test';
import { type Box, nearestPane, neighborTab } from './navigate';

const tabs = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];

test('the next and the previous tab wrap at the ends', () => {
  expect(neighborTab(tabs, 'a', 1)).toBe('b');
  expect(neighborTab(tabs, 'c', 1)).toBe('a');
  expect(neighborTab(tabs, 'a', -1)).toBe('c');
  expect(neighborTab(tabs, 'b', -1)).toBe('a');
});

test('without a current tab the first or the last tab comes', () => {
  expect(neighborTab(tabs, null, 1)).toBe('a');
  expect(neighborTab(tabs, undefined, -1)).toBe('c');
  expect(neighborTab([], 'a', 1)).toBeUndefined();
});

// Left column: A over B. Right column: C.
const grid: Box[] = [
  { id: 'A', x: 0, y: 0, w: 100, h: 50 },
  { id: 'B', x: 0, y: 50, w: 100, h: 50 },
  { id: 'C', x: 100, y: 0, w: 100, h: 100 },
];

test('a pane finds its neighbour in each direction', () => {
  expect(nearestPane(grid, 'A', 'right')).toBe('C');
  expect(nearestPane(grid, 'A', 'down')).toBe('B');
  expect(nearestPane(grid, 'B', 'up')).toBe('A');
  expect(nearestPane(grid, 'C', 'left')).toBe('A');
});

test('no pane lies past the edge: nothing is found', () => {
  expect(nearestPane(grid, 'A', 'left')).toBeUndefined();
  expect(nearestPane(grid, 'A', 'up')).toBeUndefined();
  expect(nearestPane(grid, 'C', 'right')).toBeUndefined();
  expect(nearestPane(grid, 'missing', 'right')).toBeUndefined();
});

test('a pane that lines up beats a pane at the side', () => {
  const boxes: Box[] = [
    { id: 'from', x: 0, y: 0, w: 100, h: 100 },
    { id: 'low', x: 100, y: 60, w: 100, h: 40 },
    { id: 'level', x: 100, y: 20, w: 100, h: 60 },
  ];
  expect(nearestPane(boxes, 'from', 'right')).toBe('level');
});
