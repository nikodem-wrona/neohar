// What the page tells the core about focus: which pane has the DOM focus (`active_pane`),
// whether the page has focus and is visible, whether a view covers the panes, and the refresh rate
// of the display. The host joins
// the page flags with what the window system knows. The page sends a report when something
// changed.

export interface FocusReport {
  window: string;
  focused: boolean;
  visible: boolean;
  active_pane: string | null;
  refresh_hz?: number;
  /** A view of the page covers the panes, so the window draws none. */
  panes_hidden?: boolean;
}

/** The part of an element that the lookup needs. */
export interface ElementLike {
  closest(selector: string): { getAttribute(name: string): string | null } | null;
}

/** The pane whose surface holds `element`, or `null` when the focus is in chrome. */
export function activePane(element: ElementLike | null): string | null {
  const surface = element?.closest('.pane-surface');
  if (!surface) return null;
  const pane = element?.closest('[data-pane]');
  return pane?.getAttribute('data-pane') ?? null;
}

export function sameReport(a: FocusReport | undefined, b: FocusReport): boolean {
  return (
    a !== undefined &&
    a.window === b.window &&
    a.focused === b.focused &&
    a.visible === b.visible &&
    a.active_pane === b.active_pane &&
    a.refresh_hz === b.refresh_hz &&
    a.panes_hidden === b.panes_hidden
  );
}

export interface FocusSource {
  addEventListener(type: string, listener: () => void, options?: boolean): void;
  removeEventListener(type: string, listener: () => void, options?: boolean): void;
}

/**
 * Sends a report now and after each change of focus, visibility or rate. `resend` sends it again:
 * a core that lost its client forgets the focus and the visibility of the windows. `read` gives the state of
 * the page. `refresh` is the rate once it is known.
 */
export function startFocusReport(options: {
  window: string;
  document: FocusSource & {
    activeElement: ElementLike | null;
    visibilityState: string;
    hasFocus(): boolean;
  };
  page: FocusSource;
  send: (report: FocusReport) => void;
}): {
  setRefresh(hz: number): void;
  setPanesHidden(hidden: boolean): void;
  resend(): void;
  stop(): void;
} {
  const { document: doc, page } = options;
  let hz: number | undefined;
  let hidden = false;
  let last: FocusReport | undefined;
  const report = (): void => {
    const next: FocusReport = {
      window: options.window,
      focused: doc.hasFocus(),
      visible: doc.visibilityState === 'visible',
      active_pane: activePane(doc.activeElement),
      ...(hz !== undefined ? { refresh_hz: hz } : {}),
      ...(hidden ? { panes_hidden: true } : {}),
    };
    if (sameReport(last, next)) return;
    last = next;
    options.send(next);
  };
  const events: [FocusSource, string, boolean?][] = [
    [doc, 'focusin'],
    [doc, 'focusout'],
    [doc, 'visibilitychange'],
    [page, 'focus'],
    [page, 'blur'],
  ];
  for (const [source, type] of events) source.addEventListener(type, report);
  report();
  return {
    setRefresh(value) {
      hz = value;
      report();
    },
    /** The core sends no frame while a view covers the panes, and keyframes after it. */
    setPanesHidden(value) {
      hidden = value;
      report();
    },
    /** Sends the report again, also when nothing changed. A core forgets it when a link ends. */
    resend() {
      last = undefined;
      report();
    },
    stop() {
      for (const [source, type] of events) source.removeEventListener(type, report);
    },
  };
}
