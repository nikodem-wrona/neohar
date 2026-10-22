import { expect, test } from 'bun:test';
import {
  bypassesMouseMode,
  buttonName,
  cellAt,
  modsOf,
  mouseProtocol,
  pixelsPerNotch,
  selectMode,
  WheelAccumulator,
} from './mouse';

const box = { left: 100, top: 50 };
const cell = { w: 8, h: 16 };
const grid = { cols: 80, rows: 24 };

test('a point gives the cell under it', () => {
  expect(cellAt({ x: 100, y: 50 }, box, cell, grid)).toEqual({ col: 0, row: 0 });
  expect(cellAt({ x: 107.9, y: 65.9 }, box, cell, grid)).toEqual({ col: 0, row: 0 });
  expect(cellAt({ x: 108, y: 66 }, box, cell, grid)).toEqual({ col: 1, row: 1 });
  expect(cellAt({ x: 100 + 79 * 8 + 3, y: 50 + 23 * 16 + 3 }, box, cell, grid)).toEqual({
    col: 79,
    row: 23,
  });
});

test('a point outside the box gives the nearest cell', () => {
  expect(cellAt({ x: 0, y: 0 }, box, cell, grid)).toEqual({ col: 0, row: 0 });
  expect(cellAt({ x: 5000, y: 5000 }, box, cell, grid)).toEqual({ col: 79, row: 23 });
  expect(cellAt({ x: 108, y: 5000 }, box, cell, grid)).toEqual({ col: 1, row: 23 });
});

test('the modifiers come in the order of the protocol', () => {
  expect(modsOf({ shiftKey: true, ctrlKey: true, altKey: true, metaKey: true })).toEqual([
    'shift',
    'ctrl',
    'alt',
    'meta',
  ]);
  expect(modsOf({ shiftKey: false, ctrlKey: false, altKey: true, metaKey: false })).toEqual([
    'alt',
  ]);
});

test('the buttons have names', () => {
  expect([0, 1, 2, 3].map(buttonName)).toEqual(['left', 'middle', 'right', 'none']);
});

test('clicks and Alt decide the mode of a selection', () => {
  expect(selectMode(1, false)).toBe('simple');
  expect(selectMode(2, false)).toBe('word');
  expect(selectMode(3, false)).toBe('line');
  expect(selectMode(4, false)).toBe('line');
  expect(selectMode(1, true)).toBe('block');
  expect(selectMode(2, true)).toBe('block');
});

test('Shift makes a selection although the program asked for the mouse', () => {
  expect(bypassesMouseMode({ shiftKey: true })).toBe(true);
  expect(bypassesMouseMode({ shiftKey: false })).toBe(false);
});

test('a notch is three lines of pixels', () => {
  expect(pixelsPerNotch(16)).toBe(48);
});

test('a trackpad makes a notch from many small steps', () => {
  const wheel = new WheelAccumulator();
  const step = (dy: number) => wheel.add(0, dy, 0, 16);
  // 48 pixels are one notch. The steps of 10 pixels add up.
  expect(step(10)).toEqual({ x: 0, y: 0 });
  expect(step(10)).toEqual({ x: 0, y: 0 });
  expect(step(10)).toEqual({ x: 0, y: 0 });
  expect(step(10)).toEqual({ x: 0, y: 0 });
  expect(step(10)).toEqual({ x: 0, y: 1 });
  // 2 pixels are left. A big step gives several notches.
  expect(step(100)).toEqual({ x: 0, y: 2 });
  // The sign is kept for a move up.
  expect(step(-96)).toEqual({ x: 0, y: -1 });
});

test('a wheel in lines gives a notch for each three lines, and a page a notch', () => {
  const wheel = new WheelAccumulator();
  expect(wheel.add(0, 3, 1, 16)).toEqual({ x: 0, y: 1 });
  expect(wheel.add(0, 1, 1, 16)).toEqual({ x: 0, y: 0 });
  expect(wheel.add(0, 2, 1, 16)).toEqual({ x: 0, y: 1 });
  expect(wheel.add(2, 0, 2, 16)).toEqual({ x: 2, y: 0 });
  wheel.add(0, 1, 1, 16);
  wheel.reset();
  expect(wheel.add(0, 2, 1, 16)).toEqual({ x: 0, y: 0 });
});

test('the mouse protocol comes from the mode flags, the widest one first', () => {
  expect(mouseProtocol(0)).toBe('none');
  expect(mouseProtocol(1 << 3)).toBe('x10');
  expect(mouseProtocol(1 << 4)).toBe('normal');
  expect(mouseProtocol((1 << 4) | (1 << 5))).toBe('button_event');
  expect(mouseProtocol((1 << 4) | (1 << 5) | (1 << 6))).toBe('any_event');
  // The SGR encoding and the other modes do not change it.
  expect(mouseProtocol((1 << 7) | (1 << 11))).toBe('none');
  expect(mouseProtocol((1 << 7) | (1 << 4))).toBe('normal');
});
