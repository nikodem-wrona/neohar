// The frame of a window: the command line on top, the board of the agents under it, the
// workspaces in the left column, the tab bar with the layout in the middle, the drawer on the right
// and the function keys at the bottom. The side columns have a width in characters that a drag of
// their rule changes, and each folds to a rail. A window narrower than 1280 px folds the drawer, and
// one narrower than 1000 px also folds the left column; the person can open them again. The body of
// the window waits for the first snapshots, and says so when the core does not know the window.
// The Git view (⌘⇧G) takes the place of the tab bar and the layout, and folds both side columns
// while it shows.
//
// On a phone, portrait has the command line, the board, the tab bar, one pane and the
// bottom bar. The left column, the drawer, More and the settings open as sheets, and a pane that
// gets the focus closes the left column and the drawer. Landscape has the layout of the desk with
// both columns folded.

import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { dragged } from '../app/drag';
import { loadFaces } from '../app/faces';
import { fontOf, keymapOf, optionAsAlt, themeVariables } from '../app/preferences';
import type { TypingField } from '../app/view';
import { DEFAULT_FONT_FAMILY, DEFAULT_FONT_PX } from '../render/defaults';
import { startFocusReport } from '../focus/report';
import { sampleRefresh } from '../focus/refresh';
import { revealed } from '../app/reveal';
import { LayoutView } from '../layout/LayoutView';
import { Palette } from '../palette/Palette';
import { Board } from '../board/Board';
import { Toaster } from '../kit/Toaster';
import { GitView } from '../git/GitView';
import { windowFolder } from '../files/folder';
import { CloseConfirm } from './CloseConfirm';
import { FailureBanner } from './FailureBanner';
import { HealthBanner } from './HealthBanner';
import { LinkBanner, linkIsDown } from './LinkBanner';
import { Mirror } from './Mirror';
import { type ActionContext } from '../keymap/actions';
import { installKeymap } from '../keymap/install';
import { pageFrames } from '../lib/frames';
import { useLatest, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { CommandBar } from './CommandBar';
import { Drawer, DrawerRail } from './Drawer';
import { Gutter } from './Gutter';
import { KeyBar } from '../pane/KeyBar';
import { PhoneBar } from './PhoneBar';
import { PhoneSheets } from './PhoneSheets';
import { PressMenu } from './PressMenu';
import { LeftRail, Sidebar } from './Sidebar';
import { StatusBar } from './StatusBar';
import { TabBar } from './TabBar';

/** Under this width the drawer starts folded. */
const NARROW_DRAWER = '(max-width: 1279px)';
/** Under this width the left column starts folded too. */
const NARROW_LEFT = '(max-width: 999px)';
/** A phone in portrait shows the columns as sheets. */
const PORTRAIT = '(orientation: portrait)';

/** A number for each field of a phone, so that a new field is a new key bar. */
const fieldKeys = new WeakMap<TypingField, number>();
let fieldCount = 0;
function fieldKey(field: TypingField): number {
  let key = fieldKeys.get(field);
  if (key === undefined) {
    fieldCount += 1;
    key = fieldCount;
    fieldKeys.set(field, key);
  }
  return key;
}

export interface ShellProps {
  /** More content of a section of the settings of a phone: This phone. */
  extraSettings?: (section: string) => ReactNode | undefined;
  /** More content of the left column of a phone: its machines. */
  extraLeft?: () => ReactNode;
}

export function Shell(props: ShellProps = {}) {
  const {
    stores,
    windowId,
    transport,
    run,
    report,
    view,
    render,
    keyboard,
    platform,
    form,
    toasts,
  } = useApp();
  const frame = useRef<HTMLDivElement>(null);
  const main = useRef<HTMLElement>(null);
  const shown = useSelect(stores.windows, (state) => state.byKey[windowId]);
  const windowsLoading = useSelect(stores.windows, (state) => state.status === 'loading');
  const workspaceId = shown?.workspace;
  const workspace = useSelect(stores.tree, (state) =>
    workspaceId ? state.byKey[workspaceId] : undefined,
  );
  const tab = workspace?.tabs.find((candidate) => candidate.id === shown?.tab);
  const leftOpen = useValue(view.leftOpen);
  const drawerOpen = useValue(view.drawerOpen);
  const sheet = useValue(view.sheet);
  const typing = useValue(view.typing);
  const paletteOpen = useValue(view.paletteOpen);
  const closing = useValue(view.closing);
  const columns = useValue(view.columns);
  const dragging = useValue(dragged) !== null;
  const linkDown = useSelect(stores.link, (state) => linkIsDown(state.byKey['_']?.state));
  // A phone has no Git view and no Workflows view in this version.
  const gitShown = useValue(view.gitOpen) && form !== 'phone';
  const workflowsShown = useValue(view.workflowsOpen) && form !== 'phone';
  // The Workflows view loads when it opens the first time, so the window starts without it.
  type WorkflowsModule = typeof import('../workflows/WorkflowsView');
  const [workflowsModule, setWorkflowsModule] = useState<WorkflowsModule | null>(null);
  useEffect(() => {
    if (!workflowsShown || workflowsModule) return;
    import('../workflows/WorkflowsView').then((module) => setWorkflowsModule(module), report);
  }, [workflowsShown, workflowsModule, report]);

  // What a reserved chord reads in this window. The functions read the stores when the chord
  // runs.
  const actions = useMemo<ActionContext>(() => {
    const shownNow = () => stores.windows.get().byKey[windowId];
    const workspaceNow = () => {
      const win = shownNow();
      return win?.workspace ? stores.tree.get().byKey[win.workspace] : undefined;
    };
    return {
      transport,
      run,
      report,
      windowId,
      tabs: () => workspaceNow()?.tabs ?? [],
      currentTab: () => shownNow()?.tab,
      focusedPane: () => shownNow()?.focused_pane,
      paneBoxes: () =>
        Array.from(document.querySelectorAll<HTMLElement>('.layout .pane')).map((element) => {
          const box = element.getBoundingClientRect();
          return {
            id: element.dataset.pane ?? '',
            x: box.x,
            y: box.y,
            w: box.width,
            h: box.height,
          };
        }),
      showDrawer: view.showDrawer,
      togglePalette: () => view.paletteOpen.set(!view.paletteOpen.get()),
      toggleGit: () => {
        if (form !== 'phone') view.toggleGit();
      },
      toggleWorkflows: () => {
        if (form !== 'phone') view.toggleWorkflows(windowFolder(stores, windowId) ?? null);
      },
      focusCard: view.focusCard,
    };
  }, [stores, windowId, transport, run, report, view, form]);

  // The core asks the window to show a card: a notification, the tray or the global hotkey.
  const seenReveal = useRef(0);
  const reveal = shown?.reveal;
  useEffect(() => {
    const next = revealed(seenReveal.current, reveal);
    seenReveal.current = next.seen;
    if (next.show === null) return;
    view.showDrawer('inbox');
    view.focusCard(next.show);
  }, [reveal, view]);

  // The settings change the chrome, the font, the Option key and the chords while the window runs.
  const sections = useSelect(stores.settings, (state) => state.byKey);
  const keymap = useMemo(() => keymapOf(sections, platform), [sections, platform]);
  const latestKeymap = useLatest(keymap);
  useLayoutEffect(() => {
    const variables = themeVariables(sections);
    for (const [name, color] of Object.entries(variables)) {
      document.documentElement.style.setProperty(name, color);
    }
    return () => {
      for (const name of Object.keys(variables))
        document.documentElement.style.removeProperty(name);
    };
  }, [sections]);
  // The font: the app serves the faces of the families that the webview cannot match by name,
  // and the surface measures the font once they are loaded. A change that comes
  // while the faces load wins over the one before it. The data of the chrome (codes, costs, paths)
  // is in the family of the terminal too.
  const fontRun = useRef(0);
  const loadedFamily = useRef<string | undefined>(undefined);
  const font = fontOf(sections, form);
  const family = font.family ?? DEFAULT_FONT_FAMILY;
  const px = font.px ?? DEFAULT_FONT_PX;
  useEffect(() => {
    const load = ++fontRun.current;
    loadFaces(transport.fontsUrl(family), family, px)
      .then(() => {
        if (load !== fontRun.current) return;
        const fresh = loadedFamily.current !== family;
        loadedFamily.current = family;
        render?.setFont({ family, px }, fresh);
        document.documentElement.style.setProperty('--font-data', family);
      })
      .catch(report);
  }, [family, px, transport, render, report]);
  const option = optionAsAlt(sections);
  useEffect(
    () => keyboard.setOptions({ platform, optionAsAlt: option }),
    [keyboard, platform, option],
  );

  // On a phone in portrait, the columns are sheets over the pane: a pane that gets the focus or a
  // tab that shows closes them, so that the person sees what they chose.
  const focusedPane = shown?.focused_pane;
  const shownTab = shown?.tab;
  const firstShow = useRef(true);
  useEffect(() => {
    if (firstShow.current) {
      firstShow.current = false;
      return;
    }
    if (form !== 'phone' || !window.matchMedia(PORTRAIT).matches) return;
    view.drawerOpen.set(false);
    view.leftOpen.set(false);
  }, [focusedPane, shownTab, form, view]);

  // A core that lost its client forgets that the windows have focus and are visible,
  // so the window reports again when its link comes back.
  const reporting = useRef<ReturnType<typeof startFocusReport> | undefined>(undefined);
  const wasDown = useRef(false);
  useEffect(() => {
    if (wasDown.current && !linkDown) reporting.current?.resend();
    wasDown.current = linkDown;
  }, [linkDown]);

  // The canvas of the window is the first child of the frame.
  useLayoutEffect(() => {
    const element = frame.current;
    if (!element || !render) return;
    element.prepend(render.canvas);
    return () => render.canvas.remove();
  }, [render]);

  useEffect(() => {
    if (frame.current) render?.observe(frame.current);
    // The canvas paints the main panel under the panes and the gaps between them.
    const detachFrame = main.current && render ? render.attachFrame(main.current) : undefined;
    const removeKeymap = installKeymap(
      document,
      window,
      actions,
      {
        passNext: () => view.passNext.get(),
        setPassNext: (value) => view.passNext.set(value),
      },
      () => latestKeymap.current,
    );
    const reported = startFocusReport({
      window: windowId,
      document,
      page: window,
      // A report that fails goes again with the next change, and after the link comes back.
      send: (focus) => void transport.call('window.focus_report', focus).catch(() => undefined),
    });
    reporting.current = reported;
    sampleRefresh(pageFrames, () => performance.now())
      .then((hz) => {
        if (hz !== undefined) reported.setRefresh(hz);
      })
      .catch(report);
    // A narrow window folds the drawer and then the left column, and a wide one opens them again.
    const folds = [
      { query: window.matchMedia(NARROW_DRAWER), open: view.drawerOpen },
      { query: window.matchMedia(NARROW_LEFT), open: view.leftOpen },
    ].map(({ query, open }) => {
      if (query.matches) open.set(false);
      const onWidth = (event: MediaQueryListEvent): void => {
        open.set(!event.matches);
      };
      query.addEventListener('change', onWidth);
      return () => query.removeEventListener('change', onWidth);
    });
    return () => {
      detachFrame?.();
      removeKeymap();
      reported.stop();
      reporting.current = undefined;
      for (const stop of folds) stop();
    };
  }, [actions, latestKeymap, render, report, transport, view, windowId]);
  // While the Git view or the Workflows view covers the panes, the window draws none of them.
  const panesHidden = gitShown || workflowsShown;
  useEffect(() => {
    reporting.current?.setPanesHidden(panesHidden);
  }, [panesHidden]);

  /** The body of the main panel under the tab bar. */
  const body = (): ReactNode => {
    if (windowsLoading)
      return (
        <p className="notice" role="status">
          Connecting to the core.
        </p>
      );
    if (!shown)
      return (
        <p className="notice" role="alert">
          The core does not know this window.
        </p>
      );
    if (tab)
      return (
        // The surfaces take no input while the link is down.
        <div className="main-body" inert={linkDown}>
          <LayoutView tab={tab} />
        </div>
      );
    return (
      <p className="notice" role="status">
        This window shows no tab. Make a workspace or pick a tab.
      </p>
    );
  };

  return (
    <div
      className={cn('shell', {
        phone: form === 'phone',
        'left-closed': !leftOpen,
        'drawer-closed': !drawerOpen,
        'sheet-open': sheet !== null,
        typing: typing !== null,
        dragging,
      })}
      data-window={windowId}
      style={
        {
          '--left-w': `${columns.left}ch`,
          '--drawer-w': `${columns.drawer}ch`,
        } as CSSProperties
      }
      ref={frame}
    >
      <CommandBar actions={actions} keymap={keymap} />
      <Board />
      {leftOpen ? (
        <>
          <Sidebar {...(props.extraLeft ? { extra: props.extraLeft } : {})} />
          <Gutter column="left" direction={1} label="Width of the left column" />
        </>
      ) : (
        <LeftRail />
      )}
      <main className="main" ref={main}>
        <HealthBanner />
        <LinkBanner />
        <FailureBanner />
        {/* The Git view and the Workflows view take the place of the tab bar and the panes. The
            panes keep running. */}
        {workflowsShown ? (
          <WorkflowsFrame module={workflowsModule} />
        ) : gitShown ? (
          <GitView />
        ) : (
          <>
            <TabBar />
            {body()}
          </>
        )}
      </main>
      {drawerOpen ? (
        <>
          <Gutter column="drawer" direction={-1} label="Width of the drawer" />
          <Drawer />
        </>
      ) : (
        <DrawerRail />
      )}
      <Mirror />
      {paletteOpen ? <Palette /> : null}
      {closing ? <CloseConfirm closing={closing} /> : null}
      <StatusBar actions={actions} keymap={keymap} />
      <Toaster toasts={toasts} />
      {form === 'phone' ? (
        <>
          {typing ? <KeyBar key={fieldKey(typing)} field={typing} /> : <PhoneBar />}
          <PhoneSheets
            actions={actions}
            keymap={keymap}
            {...(props.extraSettings ? { extraSettings: props.extraSettings } : {})}
          />
          <PressMenu />
        </>
      ) : null}
    </div>
  );
}

/** The Workflows view, once its chunk is there. */
function WorkflowsFrame(props: { module: typeof import('../workflows/WorkflowsView') | null }) {
  return props.module ? (
    <props.module.WorkflowsView />
  ) : (
    <p className="git-empty git-center">Opening the Workflows view…</p>
  );
}
