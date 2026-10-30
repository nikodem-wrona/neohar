import { afterEach, expect, test } from 'bun:test';
import { DRAG_THRESHOLD, dragged, isDrag, pressToDrag } from './drag';

test('a press is a drag when the pointer moves the threshold in any direction', () => {
  expect(isDrag(0, 0)).toBe(false);
  expect(isDrag(DRAG_THRESHOLD - 1, 0)).toBe(false);
  expect(isDrag(2, 2)).toBe(false);
  expect(isDrag(DRAG_THRESHOLD, 0)).toBe(true);
  expect(isDrag(0, -DRAG_THRESHOLD)).toBe(true);
  expect(isDrag(-3, 3)).toBe(true);
});

const page = globalThis as { window?: unknown; document?: unknown };
const before = { window: page.window, document: page.document };
afterEach(() => {
  page.window = before.window;
  page.document = before.document;
});

test('the dragged item is a value that follows the press, its move and its release', async () => {
  const listeners = new Map<string, (event: unknown) => void>();
  page.window = {
    addEventListener: (type: string, listener: (event: unknown) => void) =>
      listeners.set(type, listener),
    removeEventListener: (type: string) => listeners.delete(type),
  };
  page.document = { elementFromPoint: () => null };
  const seen: (string | null)[] = [];
  const stop = dragged.subscribe(() => {
    const item = dragged.get();
    seen.push(item && item.kind === 'tab' ? item.id : null);
  });
  const at = (x: number) => ({ pointerId: 1, clientX: x, clientY: 0, timeStamp: 0, type: 'x' });
  pressToDrag({ ...at(0), button: 0, pointerType: 'mouse' } as unknown as PointerEvent, {
    kind: 'tab',
    id: 'tab-1',
  });
  listeners.get('pointermove')?.(at(1));
  expect(dragged.get()).toBeNull();
  listeners.get('pointermove')?.(at(DRAG_THRESHOLD));
  expect(dragged.get()).toEqual({ kind: 'tab', id: 'tab-1' });
  listeners.get('pointercancel')?.({ ...at(DRAG_THRESHOLD), type: 'pointercancel' });
  stop();
  // The listener of the click after a drag goes in the next task.
  await new Promise((resolve) => setTimeout(resolve, 1));
  expect(seen).toEqual(['tab-1', null]);
  expect(dragged.get()).toBeNull();
});
