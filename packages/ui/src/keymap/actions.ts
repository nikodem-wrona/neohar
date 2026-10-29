// What each reserved chord does. A chord names a command of the palette. This table
// runs the commands that exist today with the methods of the core, and it keeps the chords of
// commands that do not exist yet reserved: the key is taken, and nothing happens.

import type { Transport } from '@neohar/transport';
import type { App } from '../app/context';
import type { DrawerTab } from '../app/view';
import { type Box, type Direction, nearestPane, neighborTab } from './navigate';

/** What an action reads and does in this window. */
export interface ActionContext {
  transport: Transport;
  /** Calls a method and shows its failure in the banner of the window. */
  run: App['run'];
  /** Shows a failure in the banner of the window. */
  report: App['report'];
  windowId: string;
  /** The tabs of the workspace that the window shows, and the one that it shows. */
  tabs(): { id: string }[];
  currentTab(): string | null | undefined;
  focusedPane(): string | null | undefined;
  /** The panes of the window as boxes on the screen. */
  paneBoxes(): Box[];
  showDrawer(tab: DrawerTab): void;
  /** Opens the palette, or closes it when it is open. */
  togglePalette(): void;
  /** Opens the search of the scrollback, or closes it when it is open. */
  toggleSearch(): void;
  /** Opens the Git view on the folder of the window, or closes it when it is open. */
  toggleGit(): void;
  /** Shows the chat of the agent in the pane in focus, or its terminal when the chat shows. */
  toggleChat(): void;
  /** Moves the focus to the card of an interaction in the inbox. */
  focusCard(interaction: string): void;
}

type Action = (ctx: ActionContext) => void;

const selectTab =
  (step: 1 | -1): Action =>
  (ctx) => {
    const tab = neighborTab(ctx.tabs(), ctx.currentTab(), step);
    if (tab) ctx.run('tab.select', { tab, window: ctx.windowId });
  };

const focusPane =
  (direction: Direction): Action =>
  (ctx) => {
    const from = ctx.focusedPane();
    const pane = from ? nearestPane(ctx.paneBoxes(), from, direction) : undefined;
    if (pane) ctx.run('pane.focus', { pane, window: ctx.windowId });
  };

const split =
  (direction: 'right' | 'down'): Action =>
  (ctx) => {
    const pane = ctx.focusedPane();
    if (pane) ctx.run('pane.split', { pane, direction });
  };

const ACTIONS: Readonly<Record<string, Action>> = {
  'tab.next': selectTab(1),
  'tab.previous': selectTab(-1),
  'pane.focus_left': focusPane('left'),
  'pane.focus_right': focusPane('right'),
  'pane.focus_up': focusPane('up'),
  'pane.focus_down': focusPane('down'),
  'pane.split_right': split('right'),
  'pane.split_down': split('down'),
  'window.new': (ctx) => ctx.run('window.open', {}),
  'inbox.open': (ctx) => ctx.showDrawer('inbox'),
  'palette.open': (ctx) => ctx.togglePalette(),
  // The core says which interaction is next (`interaction.next`). The window shows the
  // inbox and focuses its card.
  'interaction.answer_next': (ctx) => {
    ctx.transport
      .call('interaction.next', {})
      .then((next) => {
        if (!next.interaction) return;
        ctx.showDrawer('inbox');
        ctx.focusCard(next.interaction);
      })
      .catch(ctx.report);
  },
  'search.open': (ctx) => ctx.toggleSearch(),
  'git.open': (ctx) => ctx.toggleGit(),
  'pane.view_toggle': (ctx) => ctx.toggleChat(),
  // The peek does not exist yet. Its key stays reserved.
  'peek.sibling': () => undefined,
};

/** Runs the command of a chord. It returns `false` for a command that the table does not know. */
export function runCommand(command: string, ctx: ActionContext): boolean {
  const action = ACTIONS[command];
  if (!action) return false;
  action(ctx);
  return true;
}
