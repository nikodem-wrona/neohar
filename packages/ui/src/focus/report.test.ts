import { expect, test } from 'bun:test';
import {
  activePane,
  type ElementLike,
  type FocusReport,
  sameReport,
  startFocusReport,
} from './report';

/** An element with the attributes of its ancestors. */
function element(chain: { selector: string; attribute?: string }[]): ElementLike {
  return {
    closest(selector) {
      const found = chain.find((link) => link.selector === selector);
      return found ? { getAttribute: () => found.attribute ?? null } : null;
    },
  };
}

test('the pane of a focused surface is the active pane', () => {
  const surface = element([
    { selector: '.pane-surface' },
    { selector: '[data-pane]', attribute: 'PANE1' },
  ]);
  expect(activePane(surface)).toBe('PANE1');
});

test('focus in chrome, or nothing in focus, gives no pane', () => {
  expect(activePane(element([{ selector: '[data-pane]', attribute: 'PANE1' }]))).toBeNull();
  expect(activePane(null)).toBeNull();
});

test('a report is the same only when every field is the same', () => {
  const report: FocusReport = {
    window: 'W',
    focused: true,
    visible: true,
    active_pane: null,
  };
  expect(sameReport(undefined, report)).toBe(false);
  expect(sameReport({ ...report }, report)).toBe(true);
  expect(sameReport({ ...report, active_pane: 'P' }, report)).toBe(false);
  expect(sameReport({ ...report, refresh_hz: 60 }, report)).toBe(false);
  expect(sameReport({ ...report, panes_hidden: true }, report)).toBe(false);
});

/** A document and a page that call their listeners when the test says so. */
function fakePage() {
  const listeners = new Map<string, Set<() => void>>();
  const target = {
    addEventListener: (type: string, listener: () => void) => {
      const set = listeners.get(type) ?? new Set();
      set.add(listener);
      listeners.set(type, set);
    },
    removeEventListener: (type: string, listener: () => void) =>
      listeners.get(type)?.delete(listener),
  };
  const state = { active: null as ElementLike | null, focus: true, visibility: 'visible' };
  return {
    state,
    document: {
      ...target,
      get activeElement() {
        return state.active;
      },
      get visibilityState() {
        return state.visibility;
      },
      hasFocus: () => state.focus,
    },
    page: target,
    fire: (type: string) => listeners.get(type)?.forEach((listener) => listener()),
    count: () => [...listeners.values()].reduce((sum, set) => sum + set.size, 0),
  };
}

test('a report goes out at the start and after each change, and not twice for the same state', () => {
  const fake = fakePage();
  const sent: FocusReport[] = [];
  const reporting = startFocusReport({
    window: 'W',
    document: fake.document,
    page: fake.page,
    send: (report) => sent.push(report),
  });
  expect(sent).toHaveLength(1);
  expect(sent[0]).toEqual({ window: 'W', focused: true, visible: true, active_pane: null });

  fake.fire('focusin');
  expect(sent).toHaveLength(1);

  fake.state.active = element([
    { selector: '.pane-surface' },
    { selector: '[data-pane]', attribute: 'P2' },
  ]);
  fake.fire('focusin');
  expect(sent).toHaveLength(2);
  expect(sent[1]?.active_pane).toBe('P2');

  fake.state.focus = false;
  fake.fire('blur');
  expect(sent[2]?.focused).toBe(false);

  reporting.setRefresh(120);
  expect(sent[3]?.refresh_hz).toBe(120);

  // A view that covers the panes says so, and the next report after it leaves the flag out.
  reporting.setPanesHidden(true);
  expect(sent[4]?.panes_hidden).toBe(true);
  reporting.setPanesHidden(true);
  expect(sent).toHaveLength(5);
  reporting.setPanesHidden(false);
  expect(sent[5]?.panes_hidden).toBeUndefined();

  reporting.stop();
  expect(fake.count()).toBe(0);
});
