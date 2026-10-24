import { expect, test } from 'bun:test';
import type { Transport } from '@neohar/transport';
import { type ActionContext, runCommand } from './actions';
import { compileKeymap, DEFAULT_KEYS } from '../input/chords';
import { installKeymap } from './install';

/** A transport that keeps the calls. */
function recorder() {
  const calls: { method: string; params: unknown }[] = [];
  const transport = {
    call: (method: string, params: unknown) => {
      calls.push({ method, params });
      return Promise.resolve({});
    },
  } as unknown as Transport;
  return { calls, transport };
}

function context(overrides: Partial<ActionContext> = {}) {
  const { calls, transport } = recorder();
  const drawer: string[] = [];
  const failures: unknown[] = [];
  const ctx: ActionContext = {
    transport,
    run: (method, params) => {
      calls.push({ method, params });
    },
    report: (failure) => failures.push(failure),
    windowId: 'W',
    tabs: () => [{ id: 'T1' }, { id: 'T2' }],
    currentTab: () => 'T1',
    focusedPane: () => 'A',
    paneBoxes: () => [
      { id: 'A', x: 0, y: 0, w: 100, h: 100 },
      { id: 'B', x: 100, y: 0, w: 100, h: 100 },
    ],
    showDrawer: (tab) => drawer.push(tab),
    togglePalette: () => drawer.push('palette'),
    toggleSearch: () => drawer.push('search'),
    toggleGit: () => drawer.push('git'),
    focusCard: (id) => drawer.push(`card:${id}`),
    ...overrides,
  };
  return { ctx, calls, drawer, failures };
}

test('the tab chords select the next and the previous tab of the window', () => {
  const { ctx, calls } = context();
  expect(runCommand('tab.next', ctx)).toBe(true);
  expect(runCommand('tab.previous', ctx)).toBe(true);
  expect(calls).toEqual([
    { method: 'tab.select', params: { tab: 'T2', window: 'W' } },
    { method: 'tab.select', params: { tab: 'T2', window: 'W' } },
  ]);
});

test('the focus chords move the focus to the pane in that direction', () => {
  const { ctx, calls } = context();
  runCommand('pane.focus_right', ctx);
  runCommand('pane.focus_left', ctx);
  expect(calls).toEqual([{ method: 'pane.focus', params: { pane: 'B', window: 'W' } }]);
});

test('the split chords split the focused pane, and none focused means nothing', () => {
  const { ctx, calls } = context();
  runCommand('pane.split_right', ctx);
  runCommand('pane.split_down', ctx);
  expect(calls).toEqual([
    { method: 'pane.split', params: { pane: 'A', direction: 'right' } },
    { method: 'pane.split', params: { pane: 'A', direction: 'down' } },
  ]);
  const none = context({ focusedPane: () => null });
  runCommand('pane.split_right', none.ctx);
  runCommand('pane.focus_right', none.ctx);
  expect(none.calls).toEqual([]);
});

test('a new window, the inbox, the search, and the commands of later packages', () => {
  const { ctx, calls, drawer } = context();
  expect(runCommand('window.new', ctx)).toBe(true);
  expect(calls).toEqual([{ method: 'window.open', params: {} }]);
  expect(runCommand('inbox.open', ctx)).toBe(true);
  expect(drawer).toEqual(['inbox']);
  expect(runCommand('palette.open', ctx)).toBe(true);
  expect(drawer).toEqual(['inbox', 'palette']);
  expect(runCommand('search.open', ctx)).toBe(true);
  expect(drawer).toEqual(['inbox', 'palette', 'search']);
  expect(runCommand('git.open', ctx)).toBe(true);
  expect(drawer).toEqual(['inbox', 'palette', 'search', 'git']);
  // The key of the peek is reserved, and nothing happens.
  expect(runCommand('peek.sibling', ctx)).toBe(true);
  expect(calls).toHaveLength(1);
  expect(runCommand('no.such.command', ctx)).toBe(false);
});

/** A key event with the fields that the keymap reads. */
function key(code: string, init: Partial<KeyboardEvent> = {}) {
  const event = {
    code,
    metaKey: true,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    isComposing: false,
    keyCode: 0,
    defaultPrevented: false,
    stopped: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    stopPropagation() {
      this.stopped = true;
    },
    ...init,
  };
  return event as typeof event & KeyboardEvent;
}

function keymap(keys: Record<string, string> = DEFAULT_KEYS) {
  const { ctx, calls } = context();
  let handler: ((e: KeyboardEvent) => void) | undefined;
  let blur: (() => void) | undefined;
  let pass = false;
  const stop = installKeymap(
    {
      addEventListener: (_type, listener) => (handler = listener),
      removeEventListener: () => (handler = undefined),
    },
    {
      addEventListener: (_type, listener) => (blur = listener),
      removeEventListener: () => (blur = undefined),
    },
    ctx,
    {
      passNext: () => pass,
      setPassNext: (value) => {
        pass = value;
      },
    },
    () => compileKeymap(keys, 'macos'),
  );
  return {
    calls,
    press(event: ReturnType<typeof key>) {
      handler?.(event);
      return event;
    },
    pass: () => pass,
    blur: () => blur?.(),
    installed: () => handler !== undefined,
    stop,
  };
}

test('a reserved chord is taken and its command runs', () => {
  const k = keymap();
  const event = k.press(key('KeyD'));
  expect(event.defaultPrevented).toBe(true);
  expect(event.stopped).toBe(true);
  expect(k.calls).toEqual([{ method: 'pane.split', params: { pane: 'A', direction: 'right' } }]);
});

test('other keys and the menu chords are left to the page', () => {
  const k = keymap();
  for (const event of [
    key('KeyA', { metaKey: false }),
    key('KeyC'),
    key('KeyQ'),
    key('Comma'),
    key('KeyW'),
  ]) {
    k.press(event);
    expect(event.defaultPrevented).toBe(false);
    expect(event.stopped).toBe(false);
  }
  expect(k.calls).toEqual([]);
});

test('a key during composition is left alone', () => {
  const k = keymap();
  const event = k.press(key('KeyD', { isComposing: true }));
  expect(event.defaultPrevented).toBe(false);
  const dead = k.press(key('KeyD', { keyCode: 229 }));
  expect(dead.defaultPrevented).toBe(false);
  expect(k.calls).toEqual([]);
});

test('the pass-through chord lets the next reserved key reach the pane once', () => {
  const k = keymap();
  const arm = k.press(key('KeyV', { altKey: true }));
  expect(arm.defaultPrevented).toBe(true);
  expect(k.pass()).toBe(true);

  // A modifier alone does not use it up. The next key passes, and the one after is reserved again.
  k.press(key('MetaLeft'));
  expect(k.pass()).toBe(true);
  const through = k.press(key('KeyD'));
  expect(through.defaultPrevented).toBe(false);
  expect(k.pass()).toBe(false);
  expect(k.calls).toEqual([]);
  k.press(key('KeyD'));
  expect(k.calls).toHaveLength(1);
});

test('a lost focus of the page cancels the pass-through, and stop removes the handler', () => {
  const k = keymap();
  k.press(key('KeyV', { altKey: true }));
  k.blur();
  expect(k.pass()).toBe(false);
  expect(k.installed()).toBe(true);
  k.stop();
  expect(k.installed()).toBe(false);
});

test('a chord that the settings changed runs its command, and the old chord is free', () => {
  const k = keymap({ ...DEFAULT_KEYS, pane_split_right: 'mod+j' });
  const moved = k.press(key('KeyJ'));
  expect(moved.defaultPrevented).toBe(true);
  expect(k.calls).toEqual([{ method: 'pane.split', params: { pane: 'A', direction: 'right' } }]);
  const old = k.press(key('KeyD'));
  expect(old.defaultPrevented).toBe(false);
  expect(k.calls).toHaveLength(1);
});

test('the chord for the next answer asks the core and focuses the card it names', async () => {
  const calls: { method: string; params: unknown }[] = [];
  const transport = {
    call: (method: string, params: unknown) => {
      calls.push({ method, params });
      return Promise.resolve({ interaction: 'I1', handle: 'i1' });
    },
  } as unknown as Transport;
  const { ctx, drawer } = context({ transport });
  expect(runCommand('interaction.answer_next', ctx)).toBe(true);
  await Promise.resolve();
  await Promise.resolve();
  expect(calls).toEqual([{ method: 'interaction.next', params: {} }]);
  expect(drawer).toEqual(['inbox', 'card:I1']);
});

test('the chord for the next answer does nothing when no interaction waits', async () => {
  const transport = { call: () => Promise.resolve({}) } as unknown as Transport;
  const { ctx, drawer } = context({ transport });
  runCommand('interaction.answer_next', ctx);
  await Promise.resolve();
  await Promise.resolve();
  expect(drawer).toEqual([]);
});
