import { afterEach, describe, expect, it } from 'bun:test';
import { createViewState } from './view';

describe('the folds of the side columns', () => {
  it('opens both columns at the start', () => {
    const view = createViewState();
    expect(view.leftOpen()).toBe(true);
    expect(view.drawerOpen()).toBe(true);
  });

  it('opens a folded left column for the field of a workspace, because the field is in it', () => {
    const view = createViewState();
    view.setLeftOpen(false);
    view.pageAction('workspace.rename', { workspace: 'ws-1' });
    expect(view.leftOpen()).toBe(true);
    expect(view.renaming()).toEqual({ kind: 'workspace', id: 'ws-1', place: 'left' });
  });

  it('keeps the left column folded for the field of a tab, which is in the tab bar', () => {
    const view = createViewState();
    view.setLeftOpen(false);
    view.pageAction('tab.rename', { tab: 'tab-1' });
    expect(view.leftOpen()).toBe(false);
  });
});

describe('the widths of the side columns', () => {
  const kept = new Map<string, string>();
  const storage = {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, value: string) => void kept.set(key, value),
  };
  const global = globalThis as { localStorage?: unknown };
  const before = global.localStorage;
  afterEach(() => {
    kept.clear();
    global.localStorage = before;
  });

  it('starts the left column at 28 characters', () => {
    global.localStorage = storage;
    expect(createViewState().columns()).toEqual({ left: 28, drawer: 44 });
  });

  it('keeps a width that the person set for the monitor, under the same key', () => {
    global.localStorage = storage;
    kept.set('neohar.columns', JSON.stringify({ monitor: 46, drawer: 50 }));
    const view = createViewState();
    expect(view.columns()).toEqual({ left: 46, drawer: 50 });
    view.setColumn('left', 30);
    expect(JSON.parse(kept.get('neohar.columns') ?? '{}')).toEqual({ monitor: 30, drawer: 50 });
  });
});
