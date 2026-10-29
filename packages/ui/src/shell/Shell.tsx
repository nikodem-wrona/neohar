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
  createEffect,
  createMemo,
  type JSX,
  Match,
  on,
  onCleanup,
  onMount,
  Show,
  Switch,
} from 'solid-js';
import { useApp } from '../app/context';
import { dragged } from '../app/drag';
import { loadFaces } from '../app/faces';
import { fontOf, keymapOf, optionAsAlt, themeVariables } from '../app/preferences';
import { DEFAULT_FONT_FAMILY, DEFAULT_FONT_PX } from '../render/defaults';
import { startFocusReport } from '../focus/report';
import { sampleRefresh } from '../focus/refresh';
import { revealed } from '../app/reveal';
import { LayoutView } from '../layout/LayoutView';
import { Palette } from '../palette/Palette';
import { SearchOverlay } from '../search/SearchOverlay';
import { Board } from '../board/Board';
import { GitView } from '../git/GitView';
import { CloseConfirm } from './CloseConfirm';
import { FailureBanner } from './FailureBanner';
import { HealthBanner } from './HealthBanner';
import { LinkBanner, linkIsDown } from './LinkBanner';
import { Mirror } from './Mirror';
import { type ActionContext } from '../keymap/actions';
import { installKeymap } from '../keymap/install';
import { pageFrames } from '../lib/frames';
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

export interface ShellProps {
  /** More content of a section of the settings of a phone: This phone. */
  extraSettings?: (section: string) => JSX.Element | undefined;
  /** More content of the left column of a phone: its machines. */
  extraLeft?: () => JSX.Element;
}

export function Shell(props: ShellProps = {}) {
  const { stores, windowId, transport, run, report, view, render, keyboard, platform, form } =
    useApp();
  let frame: HTMLDivElement | undefined;
  let main: HTMLElement | undefined;
  const shown = () => stores.windows.state.byKey[windowId];
  const workspace = () => {
    const win = shown();
    return win?.workspace ? stores.tree.state.byKey[win.workspace] : undefined;
  };
  const tab = () => workspace()?.tabs.find((candidate) => candidate.id === shown()?.tab);
  // A phone has no Git view in this version.
  const gitShown = () => form !== 'phone' && view.gitOpen();

  // What a reserved chord reads in this window.
  const actions: ActionContext = {
    transport,
    run,
    report,
    windowId,
    tabs: () => workspace()?.tabs ?? [],
    currentTab: () => shown()?.tab,
    focusedPane: () => shown()?.focused_pane,
    paneBoxes: () =>
      Array.from(document.querySelectorAll<HTMLElement>('.layout .pane')).map((element) => {
        const box = element.getBoundingClientRect();
        return { id: element.dataset.pane ?? '', x: box.x, y: box.y, w: box.width, h: box.height };
      }),
    showDrawer: view.showDrawer,
    togglePalette: () => view.setPaletteOpen((open) => !open),
    toggleSearch: () => view.setSearchOpen((open) => !open),
    toggleGit: () => {
      if (form !== 'phone') view.toggleGit();
    },
    toggleChat: () => {
      const pane = shown()?.focused_pane;
      if (pane) view.toggleChat(pane);
    },
    focusCard: view.focusCard,
  };

  // The core asks the window to show a card: a notification, the tray or the global hotkey.
  let seenReveal = 0;
  createEffect(() => {
    const next = revealed(seenReveal, shown()?.reveal);
    seenReveal = next.seen;
    if (next.show === null) return;
    view.showDrawer('inbox');
    view.focusCard(next.show);
  });

  // The settings change the chrome, the font, the Option key and the chords while the window runs.
  const sections = () => stores.settings.state.byKey;
  const keymap = createMemo(() => keymapOf(sections(), platform));
  createEffect(() => {
    const variables = themeVariables(sections());
    for (const [name, color] of Object.entries(variables)) {
      document.documentElement.style.setProperty(name, color);
    }
    onCleanup(() => {
      for (const name of Object.keys(variables))
        document.documentElement.style.removeProperty(name);
    });
  });
  // The font: the app serves the faces of the families that the webview cannot match by name,
  // and the surface measures the font once they are loaded. A change that comes
  // while the faces load wins over the one before it. The data of the chrome (codes, costs, paths)
  // is in the family of the terminal too.
  let fontRun = 0;
  let loadedFamily: string | undefined;
  createEffect(() => {
    const font = fontOf(sections(), form);
    const family = font.family ?? DEFAULT_FONT_FAMILY;
    const px = font.px ?? DEFAULT_FONT_PX;
    const load = ++fontRun;
    loadFaces(transport.fontsUrl(family), family, px)
      .then(() => {
        if (load !== fontRun) return;
        const fresh = loadedFamily !== family;
        loadedFamily = family;
        render?.setFont({ family, px }, fresh);
        document.documentElement.style.setProperty('--font-data', family);
      })
      .catch(report);
  });
  createEffect(() => keyboard.setOptions({ platform, optionAsAlt: optionAsAlt(sections()) }));

  // On a phone in portrait, the columns are sheets over the pane: a pane that gets the focus or a
  // tab that shows closes them, so that the person sees what they chose.
  if (form === 'phone') {
    const portrait = window.matchMedia(PORTRAIT);
    createEffect(
      on(
        () => [shown()?.focused_pane, shown()?.tab] as const,
        () => {
          if (!portrait.matches) return;
          view.setDrawerOpen(false);
          view.setLeftOpen(false);
        },
        { defer: true },
      ),
    );
  }

  // A core that lost its client forgets that the windows have focus and are visible,
  // so the window reports again when its link comes back.
  let reporting: { resend(): void; setPanesHidden(hidden: boolean): void } | undefined;
  let wasDown = false;
  createEffect(() => {
    const down = linkIsDown(stores.link.state.byKey['_']?.state);
    if (wasDown && !down) reporting?.resend();
    wasDown = down;
  });

  onMount(() => {
    if (frame) render?.observe(frame);
    // The canvas paints the main panel under the panes and the gaps between them.
    if (main && render) onCleanup(render.attachFrame(main));
    const removeKeymap = installKeymap(document, window, actions, view, keymap);
    const reported = startFocusReport({
      window: windowId,
      document,
      page: window,
      // A report that fails goes again with the next change, and after the link comes back.
      send: (focus) => void transport.call('window.focus_report', focus).catch(() => undefined),
    });
    reporting = reported;
    // While the Git view covers the panes, the window draws none of them.
    createEffect(() => reported.setPanesHidden(gitShown()));
    sampleRefresh(pageFrames, () => performance.now())
      .then((hz) => {
        if (hz !== undefined) reported.setRefresh(hz);
      })
      .catch(report);
    // A narrow window folds the drawer and then the left column, and a wide one opens them again.
    const folds = [
      { query: window.matchMedia(NARROW_DRAWER), open: view.setDrawerOpen },
      { query: window.matchMedia(NARROW_LEFT), open: view.setLeftOpen },
    ].map(({ query, open }) => {
      if (query.matches) open(false);
      const onWidth = (event: MediaQueryListEvent): void => {
        open(!event.matches);
      };
      query.addEventListener('change', onWidth);
      return () => query.removeEventListener('change', onWidth);
    });
    onCleanup(() => {
      removeKeymap();
      reported.stop();
      for (const stop of folds) stop();
    });
  });
  return (
    <div
      class="shell"
      data-window={windowId}
      classList={{
        phone: form === 'phone',
        'left-closed': !view.leftOpen(),
        'drawer-closed': !view.drawerOpen(),
        'sheet-open': view.sheet() !== null,
        typing: view.typing() !== null,
        dragging: dragged() !== null,
      }}
      style={{
        '--left-w': `${view.columns().left}ch`,
        '--drawer-w': `${view.columns().drawer}ch`,
      }}
      ref={(element) => (frame = element)}
    >
      {render?.canvas}
      <CommandBar actions={actions} keymap={keymap} />
      <Board />
      <Show when={view.leftOpen()} fallback={<LeftRail />}>
        <Sidebar {...(props.extraLeft ? { extra: props.extraLeft } : {})} />
        <Gutter column="left" direction={1} label="Width of the left column" />
      </Show>
      <main class="main" ref={(element) => (main = element)}>
        <HealthBanner />
        <LinkBanner />
        <FailureBanner />
        {/* The Git view takes the place of the tab bar and the panes. The panes keep running. */}
        <Show when={!gitShown()} fallback={<GitView />}>
          <TabBar />
          <Switch>
            <Match when={stores.windows.state.status === 'loading'}>
              <p class="notice" role="status">
                Connecting to the core.
              </p>
            </Match>
            <Match when={!shown()}>
              <p class="notice" role="alert">
                The core does not know this window.
              </p>
            </Match>
            <Match when={tab()}>
              {(current) => (
                // The surfaces take no input while the link is down.
                <div class="main-body" inert={linkIsDown(stores.link.state.byKey['_']?.state)}>
                  <LayoutView tab={current()} />
                </div>
              )}
            </Match>
            <Match when={true}>
              <p class="notice" role="status">
                This window shows no tab. Make a workspace or pick a tab.
              </p>
            </Match>
          </Switch>
        </Show>
      </main>
      <Show when={view.drawerOpen()} fallback={<DrawerRail />}>
        <Gutter column="drawer" direction={-1} label="Width of the drawer" />
        <Drawer />
      </Show>
      <Mirror />
      <Show when={view.paletteOpen()}>
        <Palette />
      </Show>
      <Show when={view.searchOpen()}>
        <SearchOverlay />
      </Show>
      <Show when={view.closing()}>{(closing) => <CloseConfirm closing={closing()} />}</Show>
      <StatusBar actions={actions} keymap={keymap} />
      <Show when={form === 'phone'}>
        <Show when={view.typing()} keyed fallback={<PhoneBar />}>
          {(field) => <KeyBar field={field} />}
        </Show>
        <PhoneSheets
          actions={actions}
          keymap={keymap}
          {...(props.extraSettings ? { extraSettings: props.extraSettings } : {})}
        />
        <PressMenu />
      </Show>
    </div>
  );
}
