import { afterEach, describe, expect, it } from 'bun:test';
import { createViewState } from './view';

describe('the folds of the side columns', () => {
  it('opens both columns at the start', () => {
    const view = createViewState();
    expect(view.leftOpen.get()).toBe(true);
    expect(view.drawerOpen.get()).toBe(true);
  });

  it('opens a folded left column for the field of a workspace, because the field is in it', () => {
    const view = createViewState();
    view.leftOpen.set(false);
    view.pageAction('workspace.rename', { workspace: 'ws-1' });
    expect(view.leftOpen.get()).toBe(true);
    expect(view.renaming.get()).toEqual({ kind: 'workspace', id: 'ws-1', place: 'left' });
  });

  it('keeps the left column folded for the field of a tab, which is in the tab bar', () => {
    const view = createViewState();
    view.leftOpen.set(false);
    view.pageAction('tab.rename', { tab: 'tab-1' });
    expect(view.leftOpen.get()).toBe(false);
  });
});

describe('the Git view', () => {
  it('folds both columns while it shows and opens them again when it closes', () => {
    const view = createViewState();
    view.drawerOpen.set(false);
    view.openGit('/w/app');
    expect([view.gitOpen.get(), view.leftOpen.get(), view.drawerOpen.get()]).toEqual([
      true,
      false,
      false,
    ]);
    expect(view.gitTarget.get().folder).toBe('/w/app');
    view.closeGit();
    expect([view.gitOpen.get(), view.leftOpen.get(), view.drawerOpen.get()]).toEqual([
      false,
      true,
      false,
    ]);
  });

  it('takes a second request while it shows, and the command of the palette toggles it', () => {
    const view = createViewState();
    view.openGit('/w/app');
    const first = view.gitTarget.get().seq;
    view.openGit('/w/app');
    expect(view.gitTarget.get().seq).toBe(first + 1);
    view.pageAction('git.open');
    expect(view.gitOpen.get()).toBe(false);
    view.pageAction('git.open');
    expect([view.gitOpen.get(), view.gitTarget.get().folder]).toEqual([true, null]);
  });
});

describe('the Workflows view', () => {
  it('takes the place of the Git view and keeps the columns of before both', () => {
    const view = createViewState();
    view.drawerOpen.set(false);
    view.openGit('/w/app');
    view.openWorkflows('/w/app');
    expect([
      view.gitOpen.get(),
      view.workflowsOpen.get(),
      view.leftOpen.get(),
      view.drawerOpen.get(),
    ]).toEqual([false, true, false, false]);
    expect(view.workflowsTarget.get().folder).toBe('/w/app');
    view.closeWorkflows();
    expect([view.workflowsOpen.get(), view.leftOpen.get(), view.drawerOpen.get()]).toEqual([
      false,
      true,
      false,
    ]);
  });

  it('opens and closes with the command of the palette', () => {
    const view = createViewState();
    view.pageAction('workflows.open');
    expect([view.workflowsOpen.get(), view.workflowsTarget.get().folder]).toEqual([true, null]);
    view.pageAction('workflows.open');
    expect(view.workflowsOpen.get()).toBe(false);
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
    expect(createViewState().columns.get()).toEqual({ left: 28, drawer: 44 });
  });

  it('keeps a width that the person set for the monitor, under the same key', () => {
    global.localStorage = storage;
    kept.set('neohar.columns', JSON.stringify({ monitor: 46, drawer: 50 }));
    const view = createViewState();
    expect(view.columns.get()).toEqual({ left: 46, drawer: 50 });
    view.setColumn('left', 30);
    expect(JSON.parse(kept.get('neohar.columns') ?? '{}')).toEqual({ monitor: 30, drawer: 50 });
  });
});

describe('the values of the view', () => {
  it('tells a listener that a part changed', () => {
    const view = createViewState();
    const seen: boolean[] = [];
    view.leftOpen.subscribe(() => seen.push(view.leftOpen.get()));
    view.openGit();
    view.closeGit();
    expect(seen).toEqual([false, true]);
  });
});
