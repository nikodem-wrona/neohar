// The shell of the UI in WebKit against a headless core: the DOM follows the
// projections. A workspace or a split that the CLI makes shows in the page, and a click or a drag
// in the page sends its command and shows what the core answers.

import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { request as httpRequest } from 'node:http';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test';
import axe from 'axe-core';
import { decodePng, inkPixels } from './png';

const exec = promisify(execFile);

interface Info {
  ws_port: number;
  token: string;
  api_socket: string;
  nh: string;
  config_dir: string;
  /** The folder of the fake `tailscale` of the core. */
  tailscale_dir: string;
}

interface Call {
  method: string;
  params: Record<string, unknown>;
  error?: string;
}

interface NhTest {
  nhTest: {
    window: string;
    calls: () => Call[];
    call: (method: string, params: unknown) => Promise<unknown>;
    keys: () => { code: string; meta: boolean; alt: boolean; shift: boolean }[];
  };
}

interface Made {
  workspace: string;
  tab: string;
  pane: string;
}

interface Shown {
  workspace: {
    tabs: { id: string; layout: { ratios?: number[] }; floating?: { rect: Rect }[] }[];
  };
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The core, the CLI as a second client of it, and a page that shows one window. */
async function openShell(page: Page, request: APIRequestContext) {
  const info = (await (await request.get('/info')).json()) as Info;
  /** A call on the socket of the CLI. */
  const api = async <T = Record<string, string>>(
    method: string,
    params: Record<string, unknown>,
  ): Promise<T> => {
    try {
      const { stdout } = await exec(info.nh, [
        '--socket',
        info.api_socket,
        'api',
        'call',
        method,
        JSON.stringify(params),
      ]);
      return JSON.parse(stdout) as T;
    } catch (failure) {
      // The message of the core is in the output of the command, and the error shows only its line.
      const stderr = (failure as { stderr?: string }).stderr ?? '';
      throw new Error(`${method}: ${stderr.trim() || String(failure)}`, { cause: failure });
    }
  };
  await page.goto(`/page.html?ws=${info.ws_port}&token=${info.token}`);
  // The page opens its window first and then mounts.
  await page.waitForFunction(() => (window as never as Partial<NhTest>).nhTest !== undefined);
  const windowId = await page.evaluate(() => (window as never as NhTest).nhTest.window);
  /** The commands that the page sent, filtered by method. */
  const calls = async (method: string): Promise<Call[]> =>
    (await page.evaluate(() => (window as never as NhTest).nhTest.calls())).filter(
      (call) => call.method === method,
    );
  /** A workspace of the CLI, shown in this window by a click on its tab in the sidebar. */
  const makeWorkspace = async (
    name: string,
    extra: Record<string, unknown> = {},
  ): Promise<Made> => {
    const made = await api<Made>('workspace.create', { name, ...extra });
    await page.locator(`[data-side-tab="${made.tab}"]`).click();
    await expect(page.locator('.tab.selected')).toHaveAttribute('data-tab', made.tab);
    return made;
  };
  /** The keys that reached the page, by code. */
  const keys = async (code: string) =>
    (await page.evaluate(() => (window as never as NhTest).nhTest.keys())).filter(
      (key) => key.code === code,
    );
  return { info, api, windowId, calls, makeWorkspace, keys };
}

/**
 * Clicks the strip of the agent of a pane: on the board, or in the list behind the `+N` cell when
 * the runs of the tests before have filled the board.
 */
async function clickAgent(page: Page, pane: string): Promise<void> {
  const strip = page.locator(`.strips .strip[data-agent-pane="${pane}"]`);
  if ((await strip.count()) === 1) return strip.click();
  await page.locator('.board-more').click();
  await page.locator(`.board-list .strip[data-agent-pane="${pane}"]`).click();
}

async function center(locator: Locator): Promise<{ x: number; y: number }> {
  const box = await locator.boundingBox();
  if (!box) throw new Error('the element has no box');
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

async function widths(page: Page): Promise<number[]> {
  return page
    .locator('.layout .pane')
    .evaluateAll((panes) => panes.map((pane) => pane.getBoundingClientRect().width));
}

// This test runs first: it needs a core without workspaces.
test('the sidebar, the tab bar and the layout follow the projections', async ({
  page,
  request,
}) => {
  const { api, windowId, calls } = await openShell(page, request);

  // The core has a window and no workspace.
  await expect(page.locator('.notice')).toContainText('shows no tab');
  await expect(page.locator('.workspace')).toHaveCount(0);
  // Nothing waits, so the attention line is empty.
  await expect(page.locator('.commandbar .badge')).toHaveCount(0);
  // With no agent run, the board says how to start one.
  await expect(page.locator('.board-empty')).toHaveText('No agent runs. Start one in a pane.');

  // A workspace of the CLI shows in the sidebar, in the tab bar and as a pane.
  const alpha = await api<Made>('workspace.create', { name: 'Alpha' });
  await expect(page.locator('.workspace-name').first()).toContainText('Alpha');
  await expect(page.locator('.tabbar .tab')).toHaveCount(1);
  await expect(page.locator('.tab.selected')).toHaveAttribute('data-tab', alpha.tab);
  await expect(page.locator('.pane')).toHaveCount(1);
  await expect(page.locator('.pane')).toHaveAttribute('data-pane', alpha.pane);
  // The chrome shows no handles by default.
  await expect(page.locator('.pane-handle')).toHaveCount(0);
  await expect(page.locator('.tab[data-handle]')).toHaveCount(0);

  // A split of the CLI makes a second pane, and the ratios become the sizes.
  await api('pane.split', { pane: alpha.pane, direction: 'right' });
  await expect(page.locator('.pane')).toHaveCount(2);
  const [left, right] = await widths(page);
  expect(Math.abs((left ?? 0) - (right ?? 1))).toBeLessThan(4);

  // A click on "New tab" sends `tab.create`, and the new tab shows in the bar and the sidebar.
  await page.getByRole('button', { name: 'New tab' }).click();
  await expect(page.locator('.tabbar .tab')).toHaveCount(2);
  await expect(page.locator('.side-tab')).toHaveCount(2);

  // A click on a tab sends `tab.select` for this window, and the selection follows `windows`.
  const first = page.locator('.tabbar .tab').first();
  await first.click();
  await expect(first).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('.pane')).toHaveCount(2);
  const selects = await calls('tab.select');
  expect(selects).toHaveLength(1);
  expect(selects[0]?.params).toMatchObject({ window: windowId });

  // A second workspace from the button of the sidebar.
  await page.getByRole('button', { name: 'New workspace' }).click();
  await expect(page.locator('.workspace')).toHaveCount(2);

  // A click on a side tab of the first workspace shows that workspace again.
  await page.locator(`[data-workspace="${alpha.workspace}"] .side-tab`).first().click();
  await expect(page.locator('.tab.selected')).toBeVisible();
  await expect(page.locator('.tabbar .tab')).toHaveCount(2);
});

test('a window that the core does not know says so', async ({ page, request }) => {
  const info = (await (await request.get('/info')).json()) as Info;
  await page.goto(
    `/page.html?ws=${info.ws_port}&token=${info.token}&window=01JD3Q9X5K7M2N4P6R8T0V1W2Y`,
  );
  await expect(page.locator('.notice')).toContainText('does not know this window');
});

test('the left column folds to a rail and opens again, as the drawer does', async ({
  page,
  request,
}) => {
  const { makeWorkspace } = await openShell(page, request);
  await makeWorkspace('Fold');
  const paneWidth = async () => (await widths(page))[0] ?? 0;
  const before = await paneWidth();

  // The arrow of the band folds the left column. A rail with the Workspaces button takes its
  // place with no rule, and the pane gets the columns. The agents stay on the board.
  await page.getByRole('button', { name: 'Fold the left column' }).click();
  await expect(page.locator('.sidebar')).toHaveCount(0);
  await expect(page.locator('.gutter[data-column="left"]')).toHaveCount(0);
  const rail = page.getByRole('navigation', { name: 'Folded left column' });
  await expect(rail).toBeVisible();
  // The button is an icon, and its name is its tooltip.
  await expect(rail.getByRole('button')).toHaveCount(1);
  await expect(rail.getByRole('button', { name: 'Show Workspaces' })).toHaveAttribute(
    'title',
    'Workspaces',
  );
  await expect(page.getByRole('region', { name: 'Board' })).toBeVisible();
  await expect.poll(paneWidth).toBeGreaterThan(before + 100);

  // The button of the rail opens the left column again, at its width.
  await page.getByRole('button', { name: 'Show Workspaces' }).click();
  await expect(page.locator('.sidebar')).toBeVisible();
  await expect(page.locator('.left-rail')).toHaveCount(0);
  await expect.poll(async () => Math.abs((await paneWidth()) - before)).toBeLessThan(2);

  // The status bar has no switch for the left column or the drawer: their arrows and rails do it.
  const views = page.getByRole('group', { name: 'Views' });
  await expect(views.getByRole('button')).toHaveText(['Git', 'Workflows']);
});

test('the board has no fold, a new agent keeps its height, and a strip moves by drag', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  // The app gives the page no drag events of HTML, so the drag must work without them.
  await page.evaluate(() => {
    for (const type of ['dragstart', 'dragenter', 'dragover', 'dragleave', 'drop']) {
      window.addEventListener(type, (event) => event.stopImmediatePropagation(), true);
    }
  });
  await makeWorkspace('Board');
  const board = page.getByRole('region', { name: 'Board' });
  const height = async () => (await board.boundingBox())?.height ?? 0;
  const tall = await height();
  expect(tall).toBeGreaterThan(30);
  await expect(page.getByRole('button', { name: 'Fold the board' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Board', exact: true })).toHaveCount(0);

  const dir = `${tmpdir()}/nh-ui-board-${process.pid}`;
  const stop = () => exec('pkill', ['-9', '-f', '^claude 3614$']).catch(() => undefined);
  try {
    await mkdir(dir, { recursive: true });
    await writeFile(`${dir}/claude`, '#!/bin/bash\nexec -a claude /bin/sleep 3614\n', {
      mode: 0o755,
    });
    // Two agents. A new agent does not change the height of the board.
    const agents: string[] = [];
    for (const name of ['Board agent one', 'Board agent two']) {
      const made = await makeWorkspace(name, { command: { argv: [`${dir}/claude`] } });
      const strip = page.locator(`.board-list .strip[data-agent-pane="${made.pane}"]`);
      await expect(strip).toHaveCount(1, { timeout: 20_000 });
      agents.push((await strip.getAttribute('data-agent')) ?? '');
    }
    expect(await height()).toBe(tall);
    const [one, two] = agents;

    // The order of the core is the order of the strips: the CLI puts both agents first.
    await api('agent.move', { agent: one, index: 0 });
    await api('agent.move', { agent: two, index: 0 });
    const strips = page.locator('.strips .strip');
    await expect(strips.nth(0)).toHaveAttribute('data-agent', two ?? '');
    await expect(strips.nth(1)).toHaveAttribute('data-agent', one ?? '');

    // The second strip dropped on the first one goes to the first place.
    await strips.nth(1).dragTo(strips.nth(0));
    await expect.poll(async () => (await calls('agent.move')).length).toBe(1);
    expect((await calls('agent.move'))[0]?.params).toMatchObject({ agent: one, index: 0 });
    await expect(strips.nth(0)).toHaveAttribute('data-agent', one ?? '');
    await expect(strips.nth(1)).toHaveAttribute('data-agent', two ?? '');
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('a divider drag shows the shares at once and sends one command when it ends', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Split');
  await api('pane.split', { pane: made.pane, direction: 'right' });
  await expect(page.locator('.pane')).toHaveCount(2);
  await expect(page.locator('.divider')).toHaveCount(1);
  const [before] = await widths(page);

  const divider = page.locator('.divider');
  const { x, y } = await center(divider);
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 60, y, { steps: 6 });
  await page.mouse.move(x + 120, y, { steps: 6 });

  // The page shows the new shares before the pointer is up, and the core has heard nothing.
  await expect.poll(async () => (await widths(page))[0] ?? 0).toBeGreaterThan((before ?? 0) + 100);
  expect(await calls('layout.set_ratios')).toHaveLength(0);

  await page.mouse.up();
  await expect.poll(async () => (await calls('layout.set_ratios')).length).toBe(1);
  const [sent] = await calls('layout.set_ratios');
  const ratios = (sent?.params.ratios ?? []) as number[];
  expect(sent?.params.path).toEqual([]);
  expect(ratios[0]).toBeGreaterThan(0.55);

  // The core keeps the shares, and the page shows what the projection says.
  const shown = await api<Shown>('workspace.show', { workspace: made.workspace });
  const kept = shown.workspace.tabs[0]?.layout.ratios ?? [];
  expect(kept[0]).toBeCloseTo(ratios[0] ?? 0, 3);
  await expect.poll(async () => (await widths(page))[0] ?? 0).toBeGreaterThan((before ?? 0) + 100);
  expect(await calls('layout.set_ratios')).toHaveLength(1);

  // A double click equalizes the shares, and a key moves the divider by one step.
  await divider.dblclick();
  await expect.poll(async () => Math.abs(((await widths(page))[0] ?? 0) - before!)).toBeLessThan(4);
  expect(await calls('layout.equalize')).toHaveLength(1);
  await divider.focus();
  await page.keyboard.press('ArrowRight');
  await expect.poll(async () => (await calls('layout.set_ratios')).length).toBe(2);
  await expect.poll(async () => (await widths(page))[0] ?? 0).toBeGreaterThan((before ?? 0) + 4);
});

test('zoom shows one pane and a second click shows all of them again', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Zoom');
  await api('pane.split', { pane: made.pane, direction: 'down' });
  await expect(page.locator('.pane')).toHaveCount(2);

  await page.locator(`[data-pane="${made.pane}"] [data-action="zoom"]`).click();
  await expect(page.locator('.pane')).toHaveCount(1);
  await expect(page.locator('.pane')).toHaveAttribute('data-pane', made.pane);
  await expect(page.locator('.pane')).toHaveClass(/zoomed/);
  expect((await calls('layout.zoom'))[0]?.params).toMatchObject({ pane: made.pane });

  await page.locator('[data-action="zoom"]').click();
  await expect(page.locator('.pane')).toHaveCount(2);
  expect((await calls('layout.zoom'))[1]?.params).not.toHaveProperty('pane');
});

test('a floating pane moves and resizes, with one command for each frame', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Float');
  await api('pane.split', { pane: made.pane, direction: 'right' });
  await expect(page.locator('.pane')).toHaveCount(2);

  // The button in the strip of a pane floats it.
  await page.locator(`[data-pane="${made.pane}"] [data-action="float"]`).click();
  const floating = page.locator('.floating');
  await expect(floating).toHaveCount(1);
  expect((await calls('pane.float'))[0]?.params).toMatchObject({ pane: made.pane });
  const start = (await api<Shown>('workspace.show', { workspace: made.workspace })).workspace
    .tabs[0]?.floating?.[0]?.rect;
  expect(start).toBeDefined();

  // Fifty pointer events in one task are one frame, so the core gets one command.
  await page.evaluate(() => {
    const element = document.querySelector('.floating');
    const title = element?.querySelector('.pane-title');
    if (!element || !title) throw new Error('no floating pane');
    const box = title.getBoundingClientRect();
    const at = (type: string, dx: number) =>
      element.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          button: 0,
          pointerId: 1,
          clientX: box.x + 20 + dx,
          clientY: box.y + 5,
        }),
      );
    // The press goes to the title, so that the target is inside the strip.
    title.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        button: 0,
        pointerId: 1,
        clientX: box.x + 20,
        clientY: box.y + 5,
      }),
    );
    for (let i = 1; i <= 50; i++) at('pointermove', i);
  });
  await expect.poll(async () => (await calls('pane.set_float_rect')).length).toBe(1);
  await page.waitForTimeout(100);
  expect(await calls('pane.set_float_rect')).toHaveLength(1);

  // The release sends nothing more when nothing waits.
  await page.evaluate(() => {
    document
      .querySelector('.floating')
      ?.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1 }));
  });
  expect(await calls('pane.set_float_rect')).toHaveLength(1);

  // The rectangle of the projection moved to the right, and the page shows it.
  await expect
    .poll(async () => {
      const shown = await api<Shown>('workspace.show', { workspace: made.workspace });
      return shown.workspace.tabs[0]?.floating?.[0]?.rect.x ?? 0;
    })
    .toBeGreaterThan((start?.x ?? 0) + 0.001);

  // A drag with the mouse on the corner makes the pane larger.
  const sizeBefore = await floating.evaluate((el) => el.getBoundingClientRect().width);
  const corner = await center(page.locator('.float-resize'));
  await page.mouse.move(corner.x, corner.y);
  await page.mouse.down();
  await page.mouse.move(corner.x + 40, corner.y + 20, { steps: 5 });
  await page.mouse.up();
  await expect
    .poll(async () => floating.evaluate((el) => el.getBoundingClientRect().width))
    .toBeGreaterThan(sizeBefore + 20);

  // The button docks the pane again.
  await page.locator(`[data-pane="${made.pane}"] [data-action="float"]`).click();
  await expect(page.locator('.floating')).toHaveCount(0);
  await expect(page.locator('.pane')).toHaveCount(2);
});

test('tabs move by drag and a pane moves onto a tab', async ({ page, request }) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  // In the app the drag handler of the host takes every drag, and the drag events of HTML do not
  // come to the page. The page stops them here too, so a drag must work without them.
  await page.evaluate(() => {
    for (const type of ['dragstart', 'dragenter', 'dragover', 'dragleave', 'drop']) {
      window.addEventListener(type, (event) => event.stopImmediatePropagation(), true);
    }
  });
  const made = await makeWorkspace('Tabs');
  await api('tab.create', { workspace: made.workspace, name: 'Two' });
  await api('tab.create', { workspace: made.workspace, name: 'Three' });
  const names = () => page.locator('.tabbar .tab').allTextContents();
  await expect.poll(names).toHaveLength(3);
  const before = await names();

  // The last tab dropped on the first one goes to the first place.
  await page.locator('.tabbar .tab').nth(2).dragTo(page.locator('.tabbar .tab').nth(0));
  await expect.poll(async () => (await calls('tab.move')).length).toBe(1);
  expect((await calls('tab.move'))[0]?.params).toMatchObject({ index: 0 });
  await expect.poll(async () => (await names())[0]).toBe(before[2]);

  // A pane dragged from the strip onto a tab of the sidebar moves into that tab.
  const second = page.locator('.tabbar .tab').nth(1);
  const target = await second.getAttribute('data-tab');
  await page
    .locator(`[data-pane="${made.pane}"] .pane-title`)
    .dragTo(page.locator(`[data-side-tab="${target}"]`));
  await expect.poll(async () => (await calls('pane.move')).length).toBe(1);
  expect((await calls('pane.move'))[0]?.params).toMatchObject({ pane: made.pane, tab: target });

  // A pane dragged onto a tab of the tab bar moves into that tab too.
  const third = await page.locator('.tabbar .tab').nth(2).getAttribute('data-tab');
  await page
    .locator(`[data-pane="${made.pane}"] .pane-title`)
    .dragTo(page.locator(`.tabbar [data-tab="${third}"]`));
  await expect.poll(async () => (await calls('pane.move')).length).toBe(2);
  expect((await calls('pane.move'))[1]?.params).toMatchObject({ pane: made.pane, tab: third });
});

/** The pane whose surface has the DOM focus, or null. */
async function activePane(page: Page): Promise<string | null> {
  return page.evaluate(
    () =>
      document.activeElement
        ?.closest('.pane-surface')
        ?.closest('[data-pane]')
        ?.getAttribute('data-pane') ?? null,
  );
}

test('reserved chords perform their commands and other keys reach the page', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace, keys } = await openShell(page, request);
  const made = await makeWorkspace('Keys');
  await api('tab.create', { workspace: made.workspace, name: 'Other' });
  await api('pane.split', { pane: made.pane, direction: 'right' });
  const panes = page.locator('.layout .pane');
  await expect(panes).toHaveCount(2);
  await expect(page.locator('.tabbar .tab')).toHaveCount(2);

  // A click in a pane sends `pane.focus`, the core says so, and the DOM focus is in that pane.
  const first = page.locator(`[data-pane="${made.pane}"]`);
  await first.locator('.pane-surface').click();
  await expect(first).toHaveClass(/focused/);
  expect(await activePane(page)).toBe(made.pane);
  // The page reports the pane that has the DOM focus.
  await expect
    .poll(async () =>
      (await calls('window.focus_report')).some((c) => c.params.active_pane === made.pane),
    )
    .toBe(true);

  // A plain key reaches the page. The commands have not run.
  await page.keyboard.press('KeyA');
  expect(await keys('KeyA')).toHaveLength(1);
  expect(await calls('pane.split')).toHaveLength(0);

  // ⌘D splits the focused pane, and the key does not reach the page.
  await page.keyboard.press('Meta+KeyD');
  await expect(panes).toHaveCount(3);
  expect((await calls('pane.split'))[0]?.params).toMatchObject({
    pane: made.pane,
    direction: 'right',
  });
  expect(await keys('KeyD')).toHaveLength(0);

  // ⌘⌥→ moves the focus to the pane on the right, and the DOM focus follows the projection.
  const before = await activePane(page);
  await page.keyboard.press('Meta+Alt+ArrowRight');
  await expect.poll(async () => activePane(page)).not.toBe(before);
  const focusCalls = await calls('pane.focus');
  expect(focusCalls.length).toBeGreaterThan(1);
  await expect(page.locator('.layout .pane.focused')).toHaveCount(1);

  // ⌘⇧] shows the next tab of the workspace.
  const selected = () => page.locator('.tabbar .tab.selected').getAttribute('data-tab');
  const tab = await selected();
  await page.keyboard.press('Meta+Shift+BracketRight');
  await expect.poll(selected).not.toBe(tab);
  await page.keyboard.press('Meta+Shift+BracketLeft');
  await expect.poll(selected).toBe(tab);

  // ⌘⇧N asks for a new window. ⌘⇧I shows the inbox of the drawer.
  await page.keyboard.press('Meta+Shift+KeyN');
  await expect.poll(async () => (await calls('window.open')).length).toBe(1);
  await page.getByRole('button', { name: 'Fold', exact: true }).click();
  await expect(page.locator('.drawer')).toHaveCount(0);
  await page.keyboard.press('Meta+Shift+KeyI');
  await expect(page.locator('.drawer')).toHaveCount(1);
  await expect(page.getByRole('tab', { name: 'Inbox' })).toHaveAttribute('aria-selected', 'true');

  // The keys of the native menu are not taken by the page: ⌘W and ⌘, reach it.
  await page.keyboard.press('Meta+KeyW');
  await page.keyboard.press('Meta+Comma');
  expect(await keys('KeyW')).toHaveLength(1);
  expect(await keys('Comma')).toHaveLength(1);

  // ⌘⌥V lets the next reserved key through, once.
  const splits = (await calls('pane.split')).length;
  await page.keyboard.press('Meta+Alt+KeyV');
  await expect(page.locator('.status-pass')).toBeVisible();
  await page.keyboard.press('Meta+KeyD');
  await expect(page.locator('.status-pass')).toHaveCount(0);
  expect(await keys('KeyD')).toHaveLength(1);
  expect(await calls('pane.split')).toHaveLength(splits);
  await page.keyboard.press('Meta+KeyD');
  await expect.poll(async () => (await calls('pane.split')).length).toBe(splits + 1);
});

test('the page reports the rate of its display', async ({ page, request }) => {
  const { calls } = await openShell(page, request);
  await expect
    .poll(async () =>
      (await calls('window.focus_report')).some(
        (c) => typeof c.params.refresh_hz === 'number' && c.params.refresh_hz > 0,
      ),
    )
    .toBe(true);
});

/** The pixels that the canvas painted in the box of `locator`. */
async function ink(locator: Locator): Promise<number> {
  return inkPixels(decodePng(await locator.screenshot()));
}

test('the canvas paints the text of a pane and the page tells the core what it painted', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  // Ten lines of hash marks fill a large part of the pane.
  const lines = `i=0; while [ $i -lt 10 ]; do echo "################################"; i=$((i+1)); done`;
  const made = await makeWorkspace('Paint', {
    command: { argv: ['/bin/sh', '-c', `${lines}; exec sleep 600`] },
  });
  const quiet = await api<Made>('tab.create', {
    workspace: made.workspace,
    name: 'Quiet',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });

  // The page told the core what the renderer is and how big a cell is.
  const metrics = (await calls('window.report_metrics'))[0]?.params;
  expect(metrics).toMatchObject({ renderer: expect.stringMatching(/webgl2|canvas2d/) });
  expect(metrics?.cell_width_px).toBeGreaterThan(3);
  expect(metrics?.cell_height_px).toBeGreaterThan(6);

  // The pane with the hash marks has far more ink than the quiet pane, whose cursor is all.
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect.poll(() => ink(surface), { timeout: 15_000 }).toBeGreaterThan(2000);
  await page.locator(`[data-side-tab="${quiet.tab}"]`).click();
  const empty = page.locator(`[data-pane="${quiet.pane}"] .pane-surface`);
  await expect(empty).toBeVisible();
  await expect.poll(() => ink(empty), { timeout: 15_000 }).toBeLessThan(600);

  // The page acknowledged frames of both panes.
  const panes = await api<{ panes: { id: string; render_id: number }[] }>('pane.list', {});
  const renderIds = panes.panes
    .filter((pane) => pane.id === made.pane || pane.id === quiet.pane)
    .map((pane) => pane.render_id);
  expect(renderIds).toHaveLength(2);
  const acked = (await calls('render.ack')).flatMap((call) =>
    (call.params.acks as { pane: number }[]).map((ack) => ack.pane),
  );
  for (const id of renderIds) expect(acked).toContain(id);
});

test('a grid that another window sized shows the rows of the cursor, and Fit takes the size', async ({
  page,
  request,
}) => {
  const { makeWorkspace } = await openShell(page, request);
  /** A call of the UI scope from the page: the socket of the CLI cannot call these. */
  const call = <T>(method: string, params: Record<string, unknown>) =>
    page.evaluate(([m, p]) => (window as never as NhTest).nhTest.call(m as string, p), [
      method,
      params,
    ] as const) as Promise<T>;
  // The program waits for the larger grid, then writes five rows of marks just above the cursor,
  // near the bottom of a grid of 80 rows.
  const marks = '#'.repeat(40);
  const rows = [66, 67, 68, 69, 70].map((row) => `\\033[${row};1H${marks}`).join('');
  const made = await makeWorkspace('Crop', {
    command: { argv: ['/bin/sh', '-c', `sleep 4; printf '${rows}'; exec sleep 600`] },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();
  const fit = page.locator(`[data-pane="${made.pane}"] [data-action="fit"]`);
  await expect(fit).toHaveCount(0);

  // Another window that shows the tab sizes the pane larger than the box of the page, and takes
  // the lease.
  const other = (await call<{ window: string }>('window.open', { tab: made.tab })).window;
  await call('pane.resize', { pane: made.pane, cols: 200, rows: 80, window: other });
  await page.waitForTimeout(600);
  await call('pane.claim', { pane: made.pane, window: other });
  await expect(fit).toBeVisible();
  // The page shows the part of the grid around the cursor, so the marks paint.
  await expect.poll(() => ink(surface), { timeout: 15_000 }).toBeGreaterThan(1500);

  // Fit takes the size back for this window, after the debounce of the lease.
  await page.waitForTimeout(600);
  await fit.click();
  await expect(fit).toHaveCount(0);
  await call('window.close', { window: other });
});

test('the size of a pane in cells follows its box, with few resizes during a drag', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Size', {
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();

  /** The size in cells that the box allows, with the metrics that the page reported. */
  const expected = async () => {
    const metrics = (await calls('window.report_metrics')).at(-1)?.params as {
      cell_width_px: number;
      cell_height_px: number;
    };
    const box = await surface.boundingBox();
    return {
      cols: Math.floor((box?.width ?? 0) / metrics.cell_width_px),
      rows: Math.floor((box?.height ?? 0) / metrics.cell_height_px),
    };
  };
  const cells = async () => {
    const shown = await api<{ pane: { cols: number; rows: number } }>('pane.show', {
      pane: made.pane,
    });
    return { cols: shown.pane.cols, rows: shown.pane.rows };
  };

  // After the first box the core has the size of the box (within a cell of rounding).
  await expect
    .poll(async () => Math.abs((await cells()).cols - (await expected()).cols))
    .toBeLessThanOrEqual(1);

  // A drag of the window: many sizes in a short time make a few `pane.resize`, the last one last.
  const before = (await calls('pane.resize')).length;
  for (let width = 1280; width >= 900; width -= 10) {
    await page.setViewportSize({ width, height: 800 });
  }
  await expect
    .poll(async () => Math.abs((await cells()).cols - (await expected()).cols), { timeout: 10_000 })
    .toBeLessThanOrEqual(1);
  const sent = (await calls('pane.resize')).slice(before);
  expect(sent.length).toBeGreaterThanOrEqual(1);
  expect(sent.length).toBeLessThanOrEqual(11);
  const last = sent.at(-1)?.params as { cols: number; rows: number };
  expect(last.cols).toBe((await cells()).cols);
});

/** The text of the screen of a pane, as the core has it. */
async function screenOf(
  api: <T>(method: string, params: Record<string, unknown>) => Promise<T>,
  pane: string,
): Promise<string> {
  return (await api<{ text: string }>('pane.read', { pane })).text;
}

/** The centre of a cell of a pane in page coordinates, with the metrics that the page reported. */
async function cellCentre(
  page: Page,
  calls: (method: string) => Promise<Call[]>,
  pane: string,
  col: number,
  row: number,
): Promise<{ x: number; y: number }> {
  const metrics = (await calls('window.report_metrics')).at(-1)?.params as {
    cell_width_px: number;
    cell_height_px: number;
  };
  const box = await page.locator(`[data-pane="${pane}"] .pane-surface`).boundingBox();
  if (!box) throw new Error('the pane has no box');
  return {
    x: box.x + (col + 0.5) * metrics.cell_width_px,
    y: box.y + (row + 0.5) * metrics.cell_height_px,
  };
}

test('keys, text from an input method and a paste reach the program', async ({ page, request }) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  // The terminal echoes what is typed, and `cat` prints each line again.
  const made = await makeWorkspace('Typing', {
    command: { argv: ['/bin/sh', '-c', 'exec cat'] },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();

  // A click gives the keyboard to the pane, and the page keeps it.
  await surface.click();
  expect(await activePane(page)).toBe(made.pane);
  await expect(page.locator(`[data-pane="${made.pane}"] .pane-input`)).toBeFocused();

  await page.keyboard.type('hello');
  await page.keyboard.press('Enter');
  await expect
    .poll(
      async () => (await screenOf(api, made.pane)).split('\n').filter((l) => l === 'hello').length,
    )
    .toBe(2);

  // Each key has its own id, and the ids grow. A shell does not ask for key releases.
  const inputs = await calls('pane.input');
  const ids = inputs.map((call) => call.params.input_id as number);
  expect(ids.length).toBeGreaterThanOrEqual(6);
  expect(new Set(ids).size).toBe(ids.length);
  expect(ids).toEqual([...ids].sort((a, b) => a - b));
  const kinds = inputs.map((call) => (call.params.event as { kind?: string }).kind);
  expect(kinds).not.toContain('release');

  // The result of an input method is text.
  await page.evaluate(() => {
    const field = document.querySelector<HTMLTextAreaElement>('.pane-input');
    field?.dispatchEvent(new CompositionEvent('compositionend', { data: 'é日本', bubbles: true }));
  });
  await expect
    .poll(async () =>
      (await calls('pane.input')).some(
        (call) => (call.params.event as { text?: string }).text === 'é日本',
      ),
    )
    .toBe(true);
  await expect.poll(async () => screenOf(api, made.pane)).toContain('é日本');

  // A paste event sends the text to `pane.paste`.
  await page.evaluate(() => {
    const field = document.querySelector<HTMLTextAreaElement>('.pane-input');
    const data = new DataTransfer();
    data.setData('text/plain', 'pasted words');
    field?.dispatchEvent(
      new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
    );
  });
  await expect.poll(async () => (await calls('pane.paste')).length).toBe(1);
  expect((await calls('pane.paste'))[0]?.params).toMatchObject({
    pane: made.pane,
    text: 'pasted words',
  });
  await expect.poll(async () => screenOf(api, made.pane)).toContain('pasted words');
});

test('the wheel scrolls the history and a selection can be made and copied', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Select', {
    command: {
      argv: ['/bin/sh', '-c', "seq 1 300; printf 'hello world\\r\\n'; exec sleep 600"],
    },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();
  await expect.poll(async () => screenOf(api, made.pane)).toContain('hello world');
  // The first box of the pane resizes it, and the rows that the screen shows change with it.
  const box = await surface.boundingBox();
  const metrics = (await calls('window.report_metrics')).at(-1)?.params as {
    cell_height_px: number;
  };
  await expect
    .poll(async () => {
      const shown = await api<{ pane: { rows: number } }>('pane.show', { pane: made.pane });
      return Math.abs(shown.pane.rows - Math.floor((box?.height ?? 0) / metrics.cell_height_px));
    })
    .toBeLessThanOrEqual(1);
  const first = async () => Number((await screenOf(api, made.pane)).split('\n')[0]?.trim() || '0');
  const start = await first();

  // A wheel up moves the viewport up, with whole notches, and the page cancels the scroll.
  const middle = await cellCentre(page, calls, made.pane, 5, 5);
  await page.mouse.move(middle.x, middle.y);
  await page.mouse.wheel(0, -480);
  await expect.poll(first).toBeLessThan(start);
  const wheels = (await calls('pane.input')).filter(
    (call) => (call.params.event as { kind?: string }).kind === 'wheel',
  );
  expect(wheels.length).toBeGreaterThanOrEqual(1);
  expect((wheels[0]?.params.event as { wheel_y: number }).wheel_y).toBeLessThan(0);
  // Back to the end. One event of the wheel is limited, so the test turns it until the end shows.
  await expect
    .poll(async () => {
      await page.mouse.wheel(0, 2000);
      return first();
    })
    .toBe(start);

  // Find the row of the text. A double click on the second word selects the word.
  const rows = (await screenOf(api, made.pane)).split('\n');
  const row = rows.findIndex((line) => line === 'hello world');
  expect(row).toBeGreaterThanOrEqual(0);
  const word = await cellCentre(page, calls, made.pane, 8, row);
  await page.mouse.dblclick(word.x, word.y);
  await expect.poll(async () => (await calls('pane.select')).length).toBeGreaterThanOrEqual(1);
  const selected = (await calls('pane.select')).at(-1)?.params as {
    mode: string;
    anchor: { col: number; line: number };
  };
  expect(selected.mode).toBe('word');
  expect(selected.anchor.col).toBe(8);

  // ⌘C copies once a frame has brought the selection to the page. The key never reaches the program.
  await expect
    .poll(async () => {
      await page.keyboard.press('Meta+KeyC');
      return (await calls('pane.copy')).length;
    })
    .toBeGreaterThan(0);
  const copied = await api<{ text: string }>('pane.copy', { pane: made.pane, return_text: true });
  expect(copied.text).toBe('world');
  const keysSent = (await calls('pane.input')).filter(
    (call) => (call.params.event as { code?: string }).code === 'KeyC',
  );
  expect(keysSent).toHaveLength(0);

  // A drag selects cells, and the page sends the last point of each frame.
  const from = await cellCentre(page, calls, made.pane, 0, row);
  const to = await cellCentre(page, calls, made.pane, 4, row);
  const before = (await calls('pane.select')).length;
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();
  const drags = (await calls('pane.select')).slice(before);
  expect(drags.length).toBeGreaterThanOrEqual(1);
  expect(drags.length).toBeLessThanOrEqual(12);
  expect(drags.at(-1)?.params).toMatchObject({
    mode: 'simple',
    anchor: { col: 0 },
    head: { col: 4 },
  });
  await expect
    .poll(
      async () =>
        (await api<{ text: string }>('pane.copy', { pane: made.pane, return_text: true })).text,
    )
    .toBe('hello');

  // A plain click clears the selection.
  const clearsBefore = (await calls('pane.select_clear')).length;
  await page.mouse.click(middle.x, middle.y);
  await expect
    .poll(async () => (await calls('pane.select_clear')).length)
    .toBeGreaterThan(clearsBefore);
});

test('a program that asked for the mouse gets the clicks, and Shift selects', async ({
  page,
  request,
}) => {
  const { calls, makeWorkspace } = await openShell(page, request);
  // The program asks for the mouse and then fills the pane. The frame with the text has the mode.
  const fill = `i=0; while [ $i -lt 10 ]; do echo "################################"; i=$((i+1)); done`;
  const made = await makeWorkspace('Mouse', {
    command: {
      argv: ['/bin/sh', '-c', `printf '\\033[?1000h\\033[?1006h'; ${fill}; exec cat`],
    },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();
  await expect.poll(() => ink(surface), { timeout: 15_000 }).toBeGreaterThan(2000);

  const at = await cellCentre(page, calls, made.pane, 3, 2);
  await page.mouse.click(at.x, at.y);
  await expect.poll(async () => (await calls('pane.input')).length).toBeGreaterThanOrEqual(2);
  const mouse = (await calls('pane.input'))
    .map((call) => call.params.event as { type: string; kind: string; col: number; row: number })
    .filter((event) => event.type === 'mouse');
  expect(mouse.map((event) => event.kind)).toEqual(['down', 'up']);
  expect(mouse[0]).toMatchObject({ col: 3, row: 2 });
  expect(await calls('pane.select')).toHaveLength(0);

  // Shift makes a selection although the program asked for the mouse.
  const to = await cellCentre(page, calls, made.pane, 7, 0);
  const from = await cellCentre(page, calls, made.pane, 0, 0);
  await page.keyboard.down('Shift');
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 6 });
  await page.mouse.up();
  await page.keyboard.up('Shift');
  await expect.poll(async () => (await calls('pane.select')).length).toBeGreaterThanOrEqual(1);
  expect(
    (await calls('pane.input')).filter(
      (call) => (call.params.event as { type: string }).type === 'mouse',
    ),
  ).toHaveLength(2);
});

test('the scrollbar shows the history, and its thumb and track scroll', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Bar', {
    command: { argv: ['/bin/sh', '-c', 'seq 1 300; exec sleep 600'] },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();
  await expect.poll(async () => screenOf(api, made.pane)).toContain('300');
  const first = async () => Number((await screenOf(api, made.pane)).split('\n')[0]?.trim() || '0');

  // There is history, so there is a thumb. At the end it lies at the bottom of the track.
  const thumb = page.locator(`[data-pane="${made.pane}"] .scrollbar-thumb`);
  await expect(thumb).toBeVisible();
  const track = page.locator(`[data-pane="${made.pane}"] .scrollbar`);
  const trackBox = await track.boundingBox();
  const thumbBox = await thumb.boundingBox();
  expect(thumbBox && trackBox && thumbBox.y + thumbBox.height).toBeCloseTo(
    (trackBox?.y ?? 0) + (trackBox?.height ?? 0),
    0,
  );
  expect(thumbBox?.height).toBeLessThan((trackBox?.height ?? 0) / 3);
  await expect(thumb).toHaveAttribute('aria-valuenow', /\d+/);

  // A drag of the thumb to the top shows the first lines. The page sends lines, once for each frame.
  await surface.hover();
  const grab = await center(thumb);
  await page.mouse.move(grab.x, grab.y);
  await page.mouse.down();
  await page.mouse.move(grab.x, (trackBox?.y ?? 0) - 20, { steps: 10 });
  await page.mouse.up();
  await expect.poll(first).toBe(1);
  const scrolls = await calls('pane.scroll');
  expect(scrolls.length).toBeGreaterThanOrEqual(1);
  expect(scrolls.every((call) => 'top_line' in call.params)).toBe(true);
  // The thumb is at the top of the track. Its value is the absolute line on top: the lines that a
  // resize took back from the history keep their numbers, so the first line need not be 0, and
  // the thumb can stay a few pixels under the top.
  await expect
    .poll(async () => ((await thumb.boundingBox())?.y ?? 0) - (trackBox?.y ?? 0))
    .toBeLessThan(8);
  await expect(thumb).toHaveAttribute('aria-valuenow', /^\d$/);

  // A click on the track below the thumb scrolls by a page.
  const rows = (await api<{ pane: { rows: number } }>('pane.show', { pane: made.pane })).pane.rows;
  const lower = await track.boundingBox();
  await page.mouse.click((lower?.x ?? 0) + 4, (lower?.y ?? 0) + (lower?.height ?? 0) - 10);
  await expect.poll(first).toBe(1 + rows);
  expect((await calls('pane.scroll')).at(-1)?.params).toMatchObject({ delta: rows });
});

test('a pane whose program ended says so and starts again', async ({ page, request }) => {
  const { calls, makeWorkspace } = await openShell(page, request);
  const marker = `/tmp/nh-restart-${Date.now()}`;
  const made = await makeWorkspace('Exit', {
    command: {
      argv: [
        '/bin/sh',
        '-c',
        `if [ -e ${marker} ]; then rm ${marker}; exec sleep 600; else touch ${marker}; exit 3; fi`,
      ],
    },
  });
  const strip = page.locator(`[data-pane="${made.pane}"] .pane-exited`);
  await expect(strip).toContainText('The program ended with code 3.');
  // The page is attached to the pane, and the window owns its size. The old pane object must not
  // show the end of the old run again after the restart (it did, once in two runs).
  await strip.getByRole('button', { name: 'Restart' }).click();
  expect((await calls('pane.restart'))[0]?.params).toMatchObject({ pane: made.pane });
  await expect(strip).toHaveCount(0);
  await page.waitForTimeout(1000);
  await expect(strip).toHaveCount(0);
});

test('the focused pane has a text mirror for screen readers', async ({ page, request }) => {
  const { api, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Mirror', {
    command: {
      argv: ['/bin/sh', '-c', 'echo first-mirror-line; echo last-mirror-line; exec sleep 600'],
    },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect(surface).toBeVisible();
  await expect.poll(async () => screenOf(api, made.pane)).toContain('last-mirror-line');
  await surface.click();
  const mirror = page.locator('.mirror');
  await expect(mirror).toContainText('first-mirror-line');
  await expect(mirror.locator('[aria-live="polite"]')).toHaveText('last-mirror-line');
  await expect(mirror.locator('[aria-live="polite"]')).toHaveCount(1);
  // The text is off the screen: it has a size of one pixel.
  const size = await mirror.evaluate((el) => el.getBoundingClientRect().width);
  expect(size).toBeLessThanOrEqual(1);
});

test('a new font changes the cell size, and the core gets it', async ({ page, request }) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Font', {
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  await expect(page.locator(`[data-pane="${made.pane}"] .pane-surface`)).toBeVisible();
  await expect.poll(async () => (await calls('window.report_metrics')).length).toBeGreaterThan(0);
  const before = (await calls('window.report_metrics')).at(-1)?.params as { cell_width_px: number };
  const cols = async () =>
    (await api<{ pane: { cols: number } }>('pane.show', { pane: made.pane })).pane.cols;
  await expect.poll(cols).toBeGreaterThan(50);
  const wide = await cols();

  await page.evaluate(() =>
    (window as never as { nhTest: { setFont: (px: number) => void } }).nhTest.setFont(26),
  );
  await expect
    .poll(
      async () =>
        ((await calls('window.report_metrics')).at(-1)?.params as { cell_width_px: number })
          .cell_width_px,
    )
    .toBeGreaterThan(before.cell_width_px + 4);
  await expect.poll(cols).toBeLessThan(wide - 10);
});

test('the palette finds a command, runs it, and asks before it stops a program', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Palette', {
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  const panes = page.locator('.layout .pane');
  await page.locator(`[data-pane="${made.pane}"] .pane-surface`).click();
  await expect(page.locator(`[data-pane="${made.pane}"]`)).toHaveClass(/focused/);
  await expect(panes).toHaveCount(1);
  const dialog = page.getByRole('dialog', { name: 'Command palette' });

  // ⌘K opens the palette with every command, and the input has the keyboard.
  await page.keyboard.press('Meta+KeyK');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('combobox')).toBeFocused();
  await expect.poll(async () => dialog.getByRole('option').count()).toBeGreaterThan(10);
  expect((await calls('palette.query'))[0]?.params).toMatchObject({ query: '' });
  expect(await activePane(page)).toBeNull();

  // Typing asks the core, once the keys stop, and the list shows the core's order with marks.
  await page.keyboard.type('split');
  // Other tests of the core made objects that have "split" in their names, and they match too.
  await expect(dialog.getByRole('option').first()).toContainText('Split pane right');
  await expect(dialog.getByRole('option').nth(1)).toContainText('Split pane down');
  await expect(dialog.getByRole('option').first().locator('mark').first()).toHaveText('Split');
  const queries = await calls('palette.query');
  expect(queries.at(-1)?.params).toMatchObject({ query: 'split' });
  // The five keys went out as one query, and the first one was for the empty text.
  expect(queries.length).toBeLessThanOrEqual(3);

  // The arrow key picks the second command, and Enter runs it for this window.
  await page.keyboard.press('ArrowDown');
  await expect(dialog.getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Enter');
  await expect(dialog).toHaveCount(0);
  const run = (await calls('palette.execute')).at(-1)?.params;
  expect(run).toMatchObject({ command: 'pane.split_down' });
  expect(run?.window).toBeTruthy();
  await expect(panes).toHaveCount(2);
  // The pane has the keyboard again.
  await expect.poll(async () => activePane(page)).not.toBeNull();

  // Escape closes the palette without a command.
  await page.keyboard.press('Meta+KeyK');
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  const executed = (await calls('palette.execute')).length;

  // A pane with a program: the core asks, and the palette shows the question.
  await page.keyboard.press('Meta+KeyK');
  await page.keyboard.type('close pane');
  await expect(dialog.getByRole('option').first()).toContainText('Close pane');
  await page.keyboard.press('Enter');
  await expect(dialog.getByRole('alert')).toContainText('runs');
  expect((await calls('palette.execute')).length).toBe(executed + 1);
  await expect(panes).toHaveCount(2);
  await dialog.getByRole('button', { name: 'Do it anyway' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(panes).toHaveCount(1);
  expect((await calls('palette.execute')).at(-1)?.params).toMatchObject({
    args: { force: true },
  });

  // A command of the page: the inbox opens in the drawer.
  await page.getByRole('button', { name: 'Fold', exact: true }).click();
  await expect(page.locator('.drawer')).toHaveCount(0);
  await page.keyboard.press('Meta+KeyK');
  await page.keyboard.type('inbox');
  await page.keyboard.press('Enter');
  await expect(page.locator('.drawer')).toHaveCount(1);
  await expect(page.getByRole('tab', { name: 'Inbox' })).toHaveAttribute('aria-selected', 'true');

  // A click outside the palette closes it, and a click on a command runs it.
  await page.keyboard.press('Meta+KeyK');
  await expect(dialog).toBeVisible();
  await page.mouse.click(5, 400);
  await expect(dialog).toHaveCount(0);
  await page.keyboard.press('Meta+KeyK');
  await page.keyboard.type('new tab');
  // The click waits for the answer of the core to the query, not the list of the text before.
  await expect(dialog.getByRole('option').first()).toContainText('New tab');
  await dialog.getByRole('option').first().click();
  await expect(page.locator('.tabbar .tab')).toHaveCount(2);
  expect(
    (await api<{ workspaces: unknown[] }>('workspace.list', {})).workspaces.length,
  ).toBeGreaterThan(0);
});

/** The settings window: a second page against the same core. */
async function openSettings(page: Page, info: Info): Promise<Page> {
  const settings = await page.context().newPage();
  await settings.goto(`/page.html?ws=${info.ws_port}&token=${info.token}&view=settings`);
  await expect(settings.locator('.settings-nav')).toBeVisible();
  return settings;
}

test('the settings window shows the settings and the other window follows an edit', async ({
  page,
  request,
}) => {
  const { info, api, calls, makeWorkspace } = await openShell(page, request);
  await makeWorkspace('Settings', { command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] } });
  const settings = await openSettings(page, info);
  try {
    // Every section of the configuration, with the new ones for the window.
    for (const name of ['Agents', 'Approvals', 'Keys', 'Theme', 'Ui']) {
      await expect(settings.getByRole('button', { name, exact: true })).toBeVisible();
    }
    await settings.getByRole('button', { name: 'Ui', exact: true }).click();
    const size = settings.locator('[data-setting="ui.font_size"]');
    await expect(size.locator('input')).toHaveValue('13');
    await expect(size.locator('.setting-source')).toHaveText('default');
    await expect(size.getByRole('button', { name: 'Reset' })).toHaveCount(0);

    // A new size reaches the core, comes back with the source of this window, and the other window
    // measures its cells again.
    const before = (await calls('window.report_metrics')).at(-1)?.params as {
      cell_width_px: number;
    };
    await size.locator('input').fill('20');
    await size.locator('input').press('Enter');
    await expect(size.locator('.setting-source')).toHaveText('ui');
    await expect
      .poll(
        async () =>
          ((await calls('window.report_metrics')).at(-1)?.params as { cell_width_px: number })
            .cell_width_px,
      )
      .toBeGreaterThan(before.cell_width_px + 2);

    // The core refuses a wrong value, and the window shows why. The size has the type of a byte.
    await size.locator('input').fill('300');
    await size.locator('input').press('Enter');
    await expect(size.getByRole('alert')).toBeVisible();
    const option = settings.locator('[data-setting="ui.macos_option_as_alt"]');
    await option.locator('input').fill('weird');
    await option.locator('input').press('Enter');
    await expect(option.getByRole('alert')).toBeVisible();

    // The switch of the handles shows them in the tab bar and in the strip of the pane, and a
    // reset hides them again.
    const handles = settings.locator('[data-setting="ui.show_handles"]');
    await expect(handles.locator('input')).not.toBeChecked();
    await expect(page.locator('.pane-handle')).toHaveCount(0);
    await handles.locator('input').check();
    await expect(page.locator('.pane-handle')).toHaveText(/^w\d+:p\d+$/);
    await expect(page.locator('.tab.selected')).toHaveAttribute('data-handle', /^w\d+:t\d+$/);
    await handles.getByRole('button', { name: 'Reset' }).click();
    await expect(page.locator('.pane-handle')).toHaveCount(0);
    await expect(page.locator('.tab[data-handle]')).toHaveCount(0);

    // A reset gives the default back.
    await size.locator('input').fill('20');
    await size.locator('input').press('Enter');
    await size.getByRole('button', { name: 'Reset' }).click();
    await expect(size.locator('.setting-source')).toHaveText('default');
    await expect(size.locator('input')).toHaveValue('13');
    await expect
      .poll(
        async () =>
          ((await calls('window.report_metrics')).at(-1)?.params as { cell_width_px: number })
            .cell_width_px,
      )
      .toBeCloseTo(before.cell_width_px, 0);
  } finally {
    await api('config.set', { key: 'ui.font_size', value: null });
    await api('config.set', { key: 'ui.macos_option_as_alt', value: null });
    await api('config.set', { key: 'ui.show_handles', value: null });
    await settings.close();
  }
});

test('a text field of the settings writes the config one time, when the value is done', async ({
  page,
  request,
}) => {
  const { info, api } = await openShell(page, request);
  const settings = await openSettings(page, info);
  /** The writes of the font size that the settings window sent. */
  const sets = async () =>
    (
      await settings.evaluate(() =>
        (
          window as never as { nhTest: { calls(): { method: string; params: unknown }[] } }
        ).nhTest.calls(),
      )
    ).filter(
      (call) =>
        call.method === 'config.set' && (call.params as { key?: string }).key === 'ui.font_size',
    );
  try {
    await settings.getByRole('button', { name: 'Ui', exact: true }).click();
    const field = settings.locator('[data-setting="ui.font_size"] input');
    await expect(field).toHaveValue('13');

    // Each key changes the field only. The value goes to the core when the field loses the focus.
    await field.fill('');
    await field.pressSequentially('16');
    await expect(field).toHaveValue('16');
    expect(await sets()).toEqual([]);
    await field.blur();
    await expect(settings.locator('[data-setting="ui.font_size"] .setting-source')).toHaveText(
      'ui',
    );
    expect((await sets()).map((call) => call.params)).toEqual([{ key: 'ui.font_size', value: 16 }]);
  } finally {
    await api('config.set', { key: 'ui.font_size', value: null });
    await settings.close();
  }
});

test('the theme and the keys of the settings change the window that is open', async ({
  page,
  request,
}) => {
  const { info, api, makeWorkspace } = await openShell(page, request);
  await makeWorkspace('Keys', { command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] } });
  const settings = await openSettings(page, info);
  const dialog = page.getByRole('dialog', { name: 'Command palette' });
  try {
    // A colour of the theme is a custom property of the window.
    await settings.getByRole('button', { name: 'Theme', exact: true }).click();
    const accent = settings.locator('[data-setting="theme.accent"]');
    await accent.locator('input').fill('#ff0000');
    await accent.locator('input').press('Enter');
    await expect
      .poll(async () =>
        page.evaluate(() => document.documentElement.style.getPropertyValue('--accent')),
      )
      .toBe('#ff0000');
    await accent.getByRole('button', { name: 'Reset' }).click();
    await expect
      .poll(async () =>
        page.evaluate(() => document.documentElement.style.getPropertyValue('--accent')),
      )
      .toBe('#4aea7d');

    // A chord of the keys: the new chord opens the palette, and the old one is free.
    await settings.getByRole('button', { name: 'Keys', exact: true }).click();
    const chord = settings.locator('[data-setting="keys.palette_open"]');
    await chord.locator('input').fill('mod+j');
    await chord.locator('input').press('Enter');
    await expect(chord.locator('.setting-source')).toHaveText('ui');
    await page.locator('.pane-surface').first().click();
    await expect
      .poll(async () => {
        await page.keyboard.press('Meta+KeyJ');
        return dialog.count();
      })
      .toBe(1);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await page.keyboard.press('Meta+KeyK');
    await expect(dialog).toHaveCount(0);
    await chord.getByRole('button', { name: 'Reset' }).click();
    await expect(chord.locator('.setting-source')).toHaveText('default');
    await expect
      .poll(async () => {
        await page.keyboard.press('Meta+KeyK');
        return dialog.count();
      })
      .toBe(1);
  } finally {
    await api('config.set', { key: 'theme.accent', value: null });
    await api('config.set', { key: 'keys.palette_open', value: null });
    await settings.close();
  }
});

/** A free port of `127.0.0.1`. */
async function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      server.close(() => resolve(port));
    });
  });
}

/** `POST /api/pair` as a phone sends it through `tailscale serve`: the host of the tailnet. */
async function pairOverHttp(port: number, code: string): Promise<number> {
  const body = JSON.stringify({ code, name: 'Test iPhone' });
  return new Promise((resolve, reject) => {
    const sent = httpRequest(
      {
        host: '127.0.0.1',
        port,
        method: 'POST',
        path: '/api/pair',
        headers: {
          Host: 'studio.example.ts.net:8443',
          Origin: 'https://studio.example.ts.net:8443',
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(body),
        },
      },
      (answer) => {
        answer.resume();
        resolve(answer.statusCode ?? 0);
      },
    );
    sent.on('error', reject);
    sent.end(body);
  });
}

// The section Phone of the settings. The core has a fake
// `tailscale` (`tests/support/fake-tailscale.sh`): a Mac in a tailnet with HTTPS certificates.
test('the section Phone publishes with Tailscale, pairs a phone, and renames and removes it', async ({
  page,
  request,
}) => {
  const { info, api } = await openShell(page, request);
  await writeFile(
    join(info.tailscale_dir, 'status.json'),
    await readFile(
      join(
        process.cwd(),
        '..',
        '..',
        'crates',
        'nh-web',
        'tests',
        'fixtures',
        'status-running.json',
      ),
    ),
  );
  const port = await freePort();
  await api('config.set', { key: 'phone.port', value: port });
  await api('config.set', { key: 'phone.enabled', value: true });
  const settings = await page.context().newPage();
  try {
    // The fragment opens the section, as the palette command "Pair a phone" does in the app.
    await settings.goto(`/page.html?ws=${info.ws_port}&token=${info.token}&view=settings#phone`);
    const section = settings.locator('[data-phone-section]');
    await expect(section).toBeVisible();
    const summary = section.locator('.phone-summary');
    await expect(summary).toHaveText(`Listens on 127.0.0.1:${port}. No phone can reach it yet.`);
    await expect(section).toContainText('Publish with Tailscale');
    // No offer before the mapping: its QR code would lead nowhere.
    await section.getByRole('button', { name: 'Pair a phone' }).click();
    await expect(section.getByRole('alert')).toContainText('Publish with Tailscale');

    await section.getByRole('button', { name: 'Publish with Tailscale' }).click();
    await expect(summary).toHaveText(
      'Published with Tailscale at https://studio.example.ts.net:8443.',
    );
    await expect(section.getByRole('alert')).toHaveCount(0);
    const changes = async () =>
      (await readFile(join(info.tailscale_dir, 'calls'), 'utf8'))
        .split('\n')
        .filter((call) => call.startsWith('serve --'));
    expect(await changes()).toEqual([`serve --bg --yes --https=8443 http://127.0.0.1:${port}`]);

    await section.getByRole('button', { name: 'Pair a phone' }).click();
    const dialog = settings.getByRole('dialog', { name: 'Pair a phone' });
    await expect(dialog.getByRole('img', { name: 'The QR code of the pairing' })).toBeVisible();
    await expect(dialog).toContainText('The code is good for 10 minutes and for one phone.');
    const code = (await dialog.locator('.phone-code').textContent()) ?? '';
    expect(code).toMatch(/^[0-9A-Z]{4}-[0-9A-Z]{4}$/);
    expect(await pairOverHttp(port, code)).toBe(200);
    await expect(dialog).toBeHidden();

    const row = section.locator('.phone-row');
    await expect(row).toHaveCount(1);
    await expect(row.locator('.phone-name')).toHaveText('Test iPhone');
    await row.getByRole('button', { name: 'Rename' }).click();
    const field = row.getByRole('textbox', { name: 'The name of the phone' });
    await field.fill('Desk phone');
    await field.press('Enter');
    await expect(row.locator('.phone-name')).toHaveText('Desk phone');
    await row.getByRole('button', { name: 'Remove' }).click();
    await expect(section.locator('.phone-row')).toHaveCount(0);
    await expect(section).toContainText('No phone is paired.');

    await section.getByRole('button', { name: 'Unpublish' }).click();
    await expect(section.getByRole('button', { name: 'Publish with Tailscale' })).toBeVisible();
    expect((await changes()).at(-1)).toBe('serve --yes --https=8443 off');

    await settings.addScriptTag({ content: axe.source });
    const violations = await settings.evaluate(async () => {
      const result = await (
        window as never as {
          axe: { run: (context: Element) => Promise<{ violations: AxeViolation[] }> };
        }
      ).axe.run(document.querySelector('[data-phone-section]') as Element);
      return result.violations
        .filter((v) => v.impact === 'serious' || v.impact === 'critical')
        .map((v) => v.id);
    });
    expect(violations).toEqual([]);
  } finally {
    await api('config.set', { key: 'phone.enabled', value: null });
    await api('config.set', { key: 'phone.port', value: null });
    await settings.close();
  }
});

// The link of the page to its core is cut with `debug.drop_links`. The test
// runs only through `link.sock` (`NEOHAR_UI_LINK=1`, `mise run test:shell:link`), because only
// there the page has a link that comes back.
test('a link that is cut shows a banner, takes no input, and the pane draws again after it', async ({
  page,
  request,
}) => {
  test.skip(
    process.env['NEOHAR_UI_LINK'] !== '1',
    'the link is cut only in the run through link.sock',
  );
  const { info, api, makeWorkspace } = await openShell(page, request);
  const marks = (mark: string) =>
    `i=0; while [ $i -lt 10 ]; do echo "${mark.repeat(32)}"; i=$((i+1)); done`;
  const made = await makeWorkspace('Cut', {
    command: {
      argv: ['/bin/sh', '-c', `${marks('#')}; read x; ${marks('@')}; exec sleep 600`],
    },
  });
  const surface = page.locator(`[data-pane="${made.pane}"] .pane-surface`);
  await expect.poll(() => ink(surface), { timeout: 15_000 }).toBeGreaterThan(2000);
  const before = await ink(surface);
  await expect(page.locator('.banner-link')).toHaveCount(0);

  // The core closes the link. The page shows why, and its surfaces take no input.
  await api('debug.drop_links', {});
  const banner = page.locator('.banner-link');
  await expect(banner).toBeVisible();
  await expect(banner).toContainText('local');
  await expect(page.locator('.main-body')).toHaveAttribute('inert', '');

  // A new link opens by itself, and the banner goes.
  await expect(banner).toHaveCount(0, { timeout: 15_000 });
  await expect(page.locator('.main-body')).not.toHaveAttribute('inert', '');

  // The pane draws again: the program prints more after the cut, and the page paints it.
  await exec(info.nh, ['--socket', info.api_socket, 'pane', 'send-keys', made.pane, 'Enter']);
  await expect.poll(() => ink(surface), { timeout: 15_000 }).toBeGreaterThan(before + 800);
  expect(await screenOf(api, made.pane)).toContain('@'.repeat(32));
});

test('what is wrong with the core shows as a banner until it is fixed', async ({
  page,
  request,
}) => {
  const { info, api } = await openShell(page, request);
  const file = `${info.config_dir}/config.toml`;
  try {
    // A key that this version does not know is a note of the doctor.
    await mkdir(info.config_dir, { recursive: true });
    await writeFile(file, '[nope]\nx = 1\n');
    await api('config.reload', {});
    // Only the notes of the configuration: the core has a HOME of its own, so the harnesses of the
    // machine can show their own notes (not logged in) beside them.
    const banner = page.locator('.banner-warning[data-warning^="config:"]');
    await expect(banner).toContainText('nope.x');
    await expect(banner.locator('.banner-hint')).toContainText('last good configuration');
    await rm(file);
    await api('config.reload', {});
    await expect(banner).toHaveCount(0);
  } finally {
    await rm(file, { force: true });
    await api('config.reload', {});
  }
});

test('an agent in a pane has a strip on the board and a mark in the strip of the pane', async ({
  page,
  request,
}) => {
  const { makeWorkspace } = await openShell(page, request);
  // A program that is called `claude`: a script that starts `sleep` under that name.
  const dir = `${tmpdir()}/nh-ui-agent-${process.pid}`;
  const script = `${dir}/claude`;
  const stop = () => exec('pkill', ['-9', '-f', '^claude 3611$']).catch(() => undefined);
  try {
    // A deep working directory, longer than the strip of the pane has room for.
    const root = `${dir}/a-long-folder-of-work/another-long-folder/and-one-more-folder/repos/the-repo`;
    await mkdir(root, { recursive: true });
    await writeFile(script, '#!/bin/bash\nexec -a claude /bin/sleep 3611\n', { mode: 0o755 });
    const made = await makeWorkspace('Agents', { command: { argv: [script] }, root });
    // The walk finds it within a second or two. The board has a strip for it, with the harness
    // on its second line and no handle.
    const row = page.locator(`.strips .strip[data-agent-pane="${made.pane}"]`);
    await expect(row).toHaveCount(1, { timeout: 20_000 });
    await expect(row.locator('.strip-work')).toHaveText('Claude Code');
    await expect(row.locator('.strip-harness')).toHaveText('Claude Code');
    await expect(row).toHaveAccessibleName(/^Claude Code · starting.* INIT Claude Code/);
    const badge = row.locator('.agent-badge');
    await expect(badge).toHaveAttribute('title', /Claude Code · starting/);
    await expect(badge).toHaveAttribute('title', /inferred from process/);
    await expect(badge).toHaveAttribute('title', /not linked to its pane/);
    // The state comes from the process, so it is a guess, and it has the mark of one.
    await expect(badge.locator('.agent-mark').first()).toHaveText('~');
    // The strip of the pane names the harness. It is not yolo.
    const strip = page.locator(`[data-pane="${made.pane}"] .pane-harness`);
    await expect(strip).toContainText('Claude Code');
    // The strip of the pane does not repeat the badge of the board, and shows the state in four
    // letters only for WAIT and ERR.
    await expect(page.locator(`[data-pane="${made.pane}"] .agent-badge`)).toHaveCount(0);
    await expect(page.locator(`[data-pane="${made.pane}"] .pane-state`)).toHaveCount(0);
    // The strip has no working directory: the details under the button i have all of it. A click
    // on the button again closes them, and so does Escape.
    const info = page.locator(`[data-pane="${made.pane}"] [data-action="details"]`);
    const details = page.getByRole('dialog', { name: /^Details of / });
    await info.click();
    await expect(info).toHaveAttribute('aria-expanded', 'true');
    await expect(details.locator('[data-detail="folder"] dd')).toHaveText(/the-repo$/, {
      timeout: 20_000,
    });
    await expect(details.locator('.harness-badge')).toHaveText('Claude Code');
    await info.click();
    await expect(details).toBeHidden();
    await info.click();
    await expect(details).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(details).toBeHidden();
    await expect(info).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('.pane-yolo')).toHaveCount(0);
    // A click on the strip gives the pane the focus. The strip then has no bar, is not in reverse,
    // and keeps the focus fill, not Hover, under the pointer. The pane has no ring, and its strip
    // is on Hover.
    await row.click();
    const pane = page.locator(`[data-pane="${made.pane}"]`);
    await expect(pane).toHaveClass(/focused/);
    await expect(pane).toHaveCSS('outline-style', 'none');
    await expect(row).toHaveCSS('box-shadow', 'none');
    await expect(row).not.toHaveCSS('background-color', 'rgb(242, 242, 242)');
    await expect(row).not.toHaveCSS('background-color', 'rgb(38, 38, 38)');
    await expect(pane.locator('.pane-title')).toHaveCSS('background-color', 'rgb(38, 38, 38)');
    await expect(row).toHaveClass(/focused/);
    // From another workspace, a click on the row shows the tab of the pane again and focuses it.
    const other = await makeWorkspace('Elsewhere');
    await expect(page.locator(`[data-pane="${made.pane}"]`)).toHaveCount(0);
    await row.click();
    await expect(page.locator('.tab.selected')).toHaveAttribute('data-tab', made.tab);
    await expect(page.locator(`[data-pane="${made.pane}"]`)).toHaveClass(/focused/);
    await expect(page.locator(`[data-side-tab="${other.tab}"]`)).not.toHaveClass(/selected/);
    // The program ends, and the badge says so.
    await stop();
    await expect(badge).toHaveClass(/agent-exited/, { timeout: 20_000 });
    await expect(badge).toHaveAttribute('title', /ended/);
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('the runs that do not fit are behind +N, which is amber for a wait and lists every agent', async ({
  page,
  request,
}) => {
  const { info, makeWorkspace } = await openShell(page, request);
  const dir = `${tmpdir()}/nh-ui-more-${process.pid}`;
  const stop = async () => {
    await exec('pkill', ['-9', '-f', '^claude 362[0-3]$']).catch(() => undefined);
    await exec('pkill', ['-9', '-f', `${dir}/`]).catch(() => undefined);
  };
  try {
    await mkdir(dir, { recursive: true });
    const panes: string[] = [];
    for (const n of [0, 1, 2]) {
      await writeFile(`${dir}/claude${n}`, `#!/bin/bash\nexec -a claude /bin/sleep 362${n}\n`, {
        mode: 0o755,
      });
      panes.push(
        (await makeWorkspace(`More ${n}`, { command: { argv: [`${dir}/claude${n}`] } })).pane,
      );
    }
    // The core finds these runs in the process list, a second or two after they start.
    for (const pane of panes) {
      await expect(page.locator(`.board-list .strip[data-agent-pane="${pane}"]`)).toHaveCount(1, {
        timeout: 20_000,
      });
    }
    // The last run asks for a permission, as Claude Code does with its hooks, so it waits. It
    // starts last, so it is the last in the order of the core (by start time): behind the cell.
    const session = JSON.stringify({ session_id: 's-more', cwd: '/work', source: 'startup' });
    const permission = JSON.stringify({
      hook_event_name: 'PermissionRequest',
      tool_name: 'Bash',
      tool_input: { command: 'make more', description: 'more' },
      permission_mode: 'default',
    });
    await writeFile(
      `${dir}/claude3`,
      `#!/bin/bash\necho '${session}' | '${info.nh}' hook claude SessionStart\n` +
        `echo '${permission}' | '${info.nh}' hook claude PermissionRequest\n` +
        'exec -a claude /bin/sleep 3623\n',
      { mode: 0o755 },
    );
    const waiting = (await makeWorkspace('More 3', { command: { argv: [`${dir}/claude3`] } })).pane;
    // 800 px: the drawer and the left column fold, and the board has room for three strips.
    await page.setViewportSize({ width: 800, height: 720 });
    const listed = page.locator(`.board-list .strip[data-agent-pane="${waiting}"]`);
    await expect(listed).toHaveAttribute('data-code', 'WAIT', { timeout: 20_000 });

    // Three strips show, the waiting run is behind the cell, and the cell is amber.
    await expect(page.locator('.strips .strip')).toHaveCount(3);
    await expect(page.locator(`.strips .strip[data-agent-pane="${waiting}"]`)).toHaveCount(0);
    const more = page.locator('.board-more');
    await expect(more).toHaveText(/^\+\d+$/);
    await expect(more).toHaveClass(/waiting/);
    // The drawer is a rail of four icons, and the Inbox button is amber with the count.
    const drawerRail = page.getByRole('navigation', { name: 'Folded drawer' });
    await expect(drawerRail.getByRole('button')).toHaveCount(4);
    await expect(drawerRail.getByRole('button', { name: /^Show Inbox, \d+ waiting$/ })).toHaveClass(
      /hot/,
    );
    // The attention line is an amber pill, and the cap of Answer is amber.
    await expect(page.locator('.commandbar .badge')).toHaveText(/^\d+ waiting$/);
    await expect(page.locator('.fkey[data-command="interaction.answer_next"] .cap')).toHaveClass(
      /cap-wait/,
    );
    await expect(page.locator('.fkey[data-command="inbox.open"] .cap')).not.toHaveClass(/cap-wait/);

    // A click opens the list of every agent over the panes. It has no serious axe violation, and
    // Escape closes it.
    const list = page.getByRole('dialog', { name: 'Every agent' });
    const paneBox = await page.locator('.layout').boundingBox();
    await more.click();
    await expect(list).toBeVisible();
    expect(await page.locator('.layout').boundingBox()).toEqual(paneBox);
    await page.addScriptTag({ content: axe.source });
    const found = await page.evaluate(async () => {
      const result = await (
        window as never as {
          axe: { run: (context: Element) => Promise<{ violations: AxeViolation[] }> };
        }
      ).axe.run(document.querySelector('.board') as Element);
      return result.violations
        .filter((v) => v.impact === 'serious' || v.impact === 'critical')
        .map((v) => v.id);
    });
    expect(found).toEqual([]);
    await page.keyboard.press('Escape');
    await expect(list).toBeHidden();

    // A click on a row of the list shows the pane in its tab, and the list closes.
    await more.click();
    await list.locator(`.strip[data-agent-pane="${waiting}"]`).click();
    await expect(list).toBeHidden();
    await expect(page.locator(`[data-pane="${waiting}"]`)).toHaveClass(/focused/);
    // The strip of the focused agent that waits keeps its amber border: the wait is louder than
    // the focus.
    await more.click();
    const focusedRow = list.locator(`.strip[data-agent-pane="${waiting}"]`);
    await expect(focusedRow).toHaveClass(/focused/);
    await expect(focusedRow).toHaveCSS('border-top-color', 'rgb(255, 184, 49)');
    await page.keyboard.press('Escape');

    // Under 100 characters the board is one line, with narrow strips of one line.
    const board = page.getByRole('region', { name: 'Board' });
    const tall = (await board.boundingBox())?.height ?? 0;
    await page.setViewportSize({ width: 600, height: 720 });
    await expect.poll(async () => (await board.boundingBox())?.height ?? 0).toBeLessThan(tall - 10);
    await expect(page.locator('.strips .strip-meta')).toHaveCount(0);
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('the Timeline tab shows the turns of the agent in focus with its usage', async ({
  page,
  request,
}) => {
  const { info, makeWorkspace } = await openShell(page, request);
  const dir = `${tmpdir()}/nh-ui-timeline-${process.pid}`;
  const script = `${dir}/claude`;
  const transcript = `${dir}/session.jsonl`;
  const stop = () => exec('pkill', ['-9', '-f', '^claude 3612$']).catch(() => undefined);
  const at = () => new Date().toISOString();
  const line = (value: unknown) => `${JSON.stringify(value)}\n`;
  try {
    await mkdir(dir, { recursive: true });
    await writeFile(transcript, '');
    // The program tells the core about its session, as Claude Code does with its hooks, and then
    // stays alive under the name `claude`.
    const payload = JSON.stringify({
      session_id: 's-ui',
      cwd: '/work',
      source: 'startup',
      transcript_path: transcript,
      model: 'claude-opus-5-5',
    });
    await writeFile(
      script,
      `#!/bin/bash\necho '${payload}' | '${info.nh}' hook claude SessionStart\nexec -a claude /bin/sleep 3612\n`,
      { mode: 0o755 },
    );
    const made = await makeWorkspace('Timeline', { command: { argv: [script] } });
    // The agents of the tests before this one are still on the board, or behind its `+N` cell,
    // whose list has every agent. This strip has our pane.
    const row = page.locator(`.board-list .strip[data-agent-pane="${made.pane}"]`);
    await expect(row).toHaveCount(1, { timeout: 20_000 });
    // The window draws the pane before it can take the focus.
    await expect(page.locator(`[data-pane="${made.pane}"] .pane-harness`)).toContainText(
      'Claude Code',
    );
    // No agent is in focus until the pane has the focus.
    await clickAgent(page, made.pane);
    await expect(page.locator(`[data-pane="${made.pane}"]`)).toHaveClass(/focused/);
    await page.getByRole('tab', { name: 'Timeline' }).click();
    // The transcript grows: a prompt, a message with its usage, a call to a tool.
    await writeFile(
      transcript,
      line({ type: 'user', uuid: 'u1', timestamp: at(), message: { content: 'make a note' } }) +
        line({
          type: 'assistant',
          uuid: 'a1',
          timestamp: at(),
          message: {
            id: 'm1',
            model: 'claude-opus-5-5',
            content: [
              { type: 'tool_use', id: 't1', name: 'Bash', input: { command: 'touch note.txt' } },
            ],
            usage: { input_tokens: 1000, output_tokens: 20 },
          },
        }),
    );
    const items = page.locator('.timeline-item');
    await expect(items).toHaveCount(2, { timeout: 20_000 });
    await expect(page.locator('.timeline-prompt .timeline-text')).toHaveText('make a note');
    const tool = page.locator('.timeline-tool');
    await expect(tool.locator('.timeline-label')).toHaveText('Bash');
    await expect(tool.locator('.timeline-text')).toHaveText('touch note.txt');
    await expect(tool.locator('.timeline-status')).toHaveText('running');
    await expect(page.getByTestId('timeline-usage')).toContainText('1000 in · 20 out');
    // The result of the call changes the item, and adds the output.
    await writeFile(
      transcript,
      line({ type: 'user', uuid: 'u1', timestamp: at(), message: { content: 'make a note' } }) +
        line({
          type: 'assistant',
          uuid: 'a1',
          timestamp: at(),
          message: {
            id: 'm1',
            model: 'claude-opus-5-5',
            content: [
              { type: 'tool_use', id: 't1', name: 'Bash', input: { command: 'touch note.txt' } },
            ],
            usage: { input_tokens: 1000, output_tokens: 20 },
          },
        }) +
        line({
          type: 'user',
          uuid: 'r1',
          timestamp: at(),
          message: { content: [{ type: 'tool_result', tool_use_id: 't1', content: 'created' }] },
        }),
    );
    await expect(tool.locator('.timeline-status')).toHaveText('done', { timeout: 20_000 });
    await expect(page.locator('.timeline-output .timeline-text')).toHaveText('created');
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('a program that writes the clipboard waits for an answer in the pane', async ({
  page,
  request,
}) => {
  const { calls, makeWorkspace } = await openShell(page, request);
  // Two requests with OSC 52, a few seconds apart: "hello" and "again".
  const script =
    "sleep 1; printf '\\033]52;c;aGVsbG8=\\a'; sleep 4; printf '\\033]52;c;YWdhaW4=\\a'; exec sleep 600";
  const made = await makeWorkspace('Clipboard', { command: { argv: ['/bin/sh', '-c', script] } });
  const strip = page.locator(`[data-pane="${made.pane}"] .pane-clipboard`);

  // The first request shows its size and its text, and Deny answers it.
  await expect(strip).toBeVisible({ timeout: 20_000 });
  await expect(strip).toContainText('5 bytes');
  await expect(strip.locator('.clipboard-preview')).toHaveText('hello');
  await strip.getByRole('button', { name: 'Deny' }).click();
  await expect(strip).toHaveCount(0);
  expect((await calls('pane.clipboard_allow')).at(-1)?.params).toMatchObject({
    pane: made.pane,
    allow: false,
    remember: false,
  });

  // The second request comes later. Allow, with the memory of the pane.
  await expect(strip.locator('.clipboard-preview')).toHaveText('again', { timeout: 20_000 });
  await strip.getByLabel('Remember for this pane').check();
  await strip.getByRole('button', { name: 'Allow' }).click();
  await expect(strip).toHaveCount(0);
  expect((await calls('pane.clipboard_allow')).at(-1)?.params).toMatchObject({
    pane: made.pane,
    allow: true,
    remember: true,
  });
});

test('the shell has no serious accessibility violation (axe smoke test)', async ({
  page,
  request,
}) => {
  const { api, makeWorkspace } = await openShell(page, request);
  const made = await makeWorkspace('Axe', {
    command: { argv: ['/bin/sh', '-c', 'echo hello; exec sleep 600'] },
  });
  await expect(page.locator('.pane')).toHaveCount(1);
  await page.addScriptTag({ content: axe.source });
  /** The violations of the page now that matter: the ones that axe calls serious or critical. */
  const violations = async () => {
    const result = await page.evaluate(async () => {
      const found = await (
        window as never as {
          axe: { run: (context: Document) => Promise<{ violations: AxeViolation[] }> };
        }
      ).axe.run(document);
      return found.violations.map((v) => ({
        id: v.id,
        impact: v.impact,
        nodes: v.nodes.map((n) => n.target.join(' ')),
      }));
    });
    return result.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  };
  // The drawer shows its four tabs, one after the other.
  for (const name of ['Inbox', 'Timeline', 'Files', 'Changes']) {
    await page.getByRole('tab', { name }).click();
    expect(await violations(), `the ${name} tab`).toEqual([]);
  }
  // The palette is a dialog over the window.
  await page.keyboard.press('Meta+KeyK');
  await expect(page.getByRole('dialog', { name: 'Command palette' })).toBeVisible();
  expect(await violations(), 'the palette').toEqual([]);
  await page.keyboard.press('Escape');
  // The question before a tab closes with a program in it.
  await expect
    .poll(async () => (await api<Alive>('pane.show', { pane: made.pane })).pane.liveness.state)
    .toBe('alive');
  await page.locator('.tab.selected .tab-close').click();
  await expect(page.getByRole('alertdialog')).toBeVisible();
  expect(await violations(), 'the question of a close').toEqual([]);
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
  // The field of a new name lies over the tab, and not in the list of the tabs.
  await page.locator('.tab.selected').dblclick();
  await expect(page.locator('.tab-rename')).toBeFocused();
  expect(await violations(), 'the field of a new name').toEqual([]);
  await page.keyboard.press('Escape');
  await expect(page.locator('.tab-rename')).toHaveCount(0);
});

test('a pane closes from the cross of its strip after a question', async ({ page, request }) => {
  const { api, makeWorkspace, calls } = await openShell(page, request);
  const command = { argv: ['/bin/sh', '-c', 'sleep 600 & exec sleep 600'] };
  const made = await makeWorkspace('Panes', { command });
  const split = await api<{ pane: string; handle: string }>('pane.split', {
    pane: made.pane,
    direction: 'right',
    command,
  });
  await expect(page.locator('.layout .pane')).toHaveCount(2);
  // A pane whose child does not run yet closes without a question, so the test waits for it.
  await expect
    .poll(async () => (await api<Alive>('pane.show', { pane: split.pane })).pane.liveness.state)
    .toBe('alive');
  const doomed = page.locator(`.pane[data-pane="${split.pane}"]`);
  const dialog = page.getByRole('alertdialog');

  // The cross asks first, and Cancel keeps the pane.
  await doomed.getByRole('button', { name: 'Close pane' }).click();
  // The handles do not show, so the dialog names the pane by its title.
  const title = (await api<{ pane: { title: string } }>('pane.show', { pane: split.pane })).pane
    .title;
  await expect(dialog).toContainText(`Close the pane “${title || split.handle}”?`);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('.layout .pane')).toHaveCount(2);

  // "Close pane" sends the close again with force, and the other pane fills the tab.
  await doomed.getByRole('button', { name: 'Close pane' }).click();
  await dialog.getByRole('button', { name: 'Close pane' }).click();
  await expect(page.locator('.layout .pane')).toHaveCount(1);
  await expect(page.locator(`.pane[data-pane="${made.pane}"]`)).toBeVisible();
  const closes = await calls('pane.close');
  expect(closes.map((call) => [call.params.pane, call.params.force])).toEqual([
    [split.pane, false],
    [split.pane, false],
    [split.pane, true],
  ]);
});

/** The part of `pane.show` that says whether the child runs. */
interface Alive {
  pane: { liveness: { state: string } };
}

test('a tab and a workspace close from their cross after a question', async ({ page, request }) => {
  const { api, makeWorkspace, calls } = await openShell(page, request);
  const command = { argv: ['/bin/sh', '-c', 'sleep 600 & exec sleep 600'] };
  const made = await makeWorkspace('Doomed', { command });
  const second = await api<{ tab: string; pane: string }>('tab.create', {
    workspace: made.workspace,
    command,
  });
  const sideTabs = page.locator(`[data-workspace="${made.workspace}"] .side-tab`);
  await expect(sideTabs).toHaveCount(2);
  // A pane whose child does not run yet closes without a question, so the test waits for both.
  for (const pane of [made.pane, second.pane]) {
    await expect
      .poll(async () => (await api<Alive>('pane.show', { pane })).pane.liveness.state)
      .toBe('alive');
  }
  await page.locator(`[data-side-tab="${second.tab}"]`).click();
  await expect(page.locator('.tab.selected')).toHaveAttribute('data-tab', second.tab);
  const dialog = page.getByRole('alertdialog');

  // The cross asks first, and Cancel keeps the tab.
  await page.locator('.tab.selected .tab-close').click();
  await expect(dialog).toContainText('Close the tab');
  await expect(dialog).toContainText('Every process that runs in it stops.');
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(sideTabs).toHaveCount(2);

  // Delete on the tab asks too, and Escape keeps the tab. The question is a modal `<dialog>`, so
  // the focus goes back to the tab when it closes.
  await page.locator('.tab.selected').focus();
  await page.keyboard.press('Delete');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Close tab' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(sideTabs).toHaveCount(2);
  await expect(page.locator('.tab.selected')).toBeFocused();

  // "Close tab" sends the close again with force, and the tab goes.
  await page.locator('.tab.selected .tab-close').click();
  await dialog.getByRole('button', { name: 'Close tab' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(sideTabs).toHaveCount(1);
  await expect(page.locator(`[data-tab="${second.tab}"]`)).toHaveCount(0);
  const closes = await calls('tab.close');
  expect(closes.map((call) => call.params.force)).toEqual([false, false, false, true]);

  // The cross of the workspace in the sidebar asks, and closes the workspace with its tab.
  const row = page.locator(`[data-workspace="${made.workspace}"]`);
  await row.locator('.workspace-name').hover();
  await row.getByRole('button', { name: 'Close workspace Doomed' }).click();
  await expect(dialog).toContainText('Close the workspace “Doomed”?');
  await dialog.getByRole('button', { name: 'Close workspace' }).click();
  await expect(row).toHaveCount(0);
  const shown = await api<{ workspaces: { id: string }[] }>('workspace.list', {});
  expect(shown.workspaces.map((ws) => ws.id)).not.toContain(made.workspace);
});

test('a workspace and a tab get a new name in place', async ({ page, request }) => {
  const { api, makeWorkspace, calls } = await openShell(page, request);
  const made = await makeWorkspace('Before');
  const row = page.locator(`[data-workspace="${made.workspace}"]`);
  const sideTab = page.locator(`[data-side-tab="${made.tab}"]`);
  const barTab = page.locator(`.tab[data-tab="${made.tab}"]`);

  // A double click on the name of the workspace opens a field with the name selected, and Enter
  // sends the new name.
  await row.locator('.workspace-title').dblclick();
  const field = row.locator('.rename-field');
  await expect(field).toBeFocused();
  await expect(field).toHaveValue('Before');
  await page.keyboard.type('After');
  await page.keyboard.press('Enter');
  await expect(field).toHaveCount(0);
  await expect(row.locator('.workspace-title')).toHaveText('After');

  // A double click on the tab of the bar opens the field over the tab. The spaces around the name
  // go.
  const over = page.locator('.tab-rename');
  await barTab.dblclick();
  await expect(over).toBeFocused();
  await over.fill('  Logs ');
  await over.press('Enter');
  await expect(barTab.locator('.tab-name')).toHaveText('Logs');
  await expect(sideTab).toHaveText('Logs');

  // F2 on the tab of the left column opens its field, and Escape keeps the name.
  await sideTab.focus();
  await page.keyboard.press('F2');
  const sideField = page.locator('.side-tab-row .rename-field');
  await expect(sideField).toBeFocused();
  await sideField.fill('Never');
  await sideField.press('Escape');
  await expect(sideField).toHaveCount(0);
  await expect(sideTab).toHaveText('Logs');

  // The palette renames the tab that the window shows, and a click outside the field sends it.
  await page.keyboard.press('Meta+KeyK');
  await page.keyboard.type('rename tab');
  await expect(page.locator('.palette-item.selected .palette-title')).toHaveText('Rename tab');
  await page.keyboard.press('Enter');
  await expect(over).toBeFocused();
  await over.fill('Build');
  await page.locator('.band-title').first().click();
  await expect(over).toHaveCount(0);
  await expect(sideTab).toHaveText('Build');

  // A name that the core refuses keeps the field open after Enter, in red, with the reason.
  await barTab.dblclick();
  await over.evaluate((input: HTMLInputElement) => {
    input.value = 'a\tb';
  });
  await over.press('Enter');
  await expect(over).toHaveAttribute('aria-invalid', 'true');
  await expect(over).toHaveAttribute('title', /control character/);
  await over.press('Escape');
  await expect(over).toHaveCount(0);
  await expect(sideTab).toHaveText('Build');

  // Enter on the old name sends nothing.
  await row.locator('.workspace-title').dblclick();
  await expect(field).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(field).toHaveCount(0);

  expect((await calls('workspace.rename')).map((call) => call.params.name)).toEqual(['After']);
  expect((await calls('tab.rename')).map((call) => call.params.name)).toEqual([
    'Logs',
    'Build',
    'a\tb',
  ]);
  const shown = await api<{ workspace: { name: string; tabs: { name: string }[] } }>(
    'workspace.show',
    { workspace: made.workspace },
  );
  expect(shown.workspace.name).toBe('After');
  expect(shown.workspace.tabs.map((tab) => tab.name)).toEqual(['Build']);
});

interface AxeViolation {
  id: string;
  impact: string | null;
  nodes: { target: unknown[] }[];
}

test('an approval card shows what the agent sent as text, and the Inbox has no accessibility violation', async ({
  page,
  request,
}) => {
  const { info, makeWorkspace } = await openShell(page, request);
  const dir = `${tmpdir()}/nh-ui-xss-${process.pid}`;
  const script = `${dir}/claude`;
  const stop = () => exec('pkill', ['-9', '-f', '^claude 3613$']).catch(() => undefined);
  try {
    await mkdir(dir, { recursive: true });
    // The agent is hostile: its command, its description and the name of its tool hold markup and
    // script. The program tells the core about its session and then asks for a permission, as
    // Claude Code does with its hooks. The shim waits for an answer, and the card shows the ask.
    const session = JSON.stringify({
      session_id: 's-xss',
      cwd: '/work',
      source: 'startup',
      model: 'claude-opus-5-5',
    });
    const permission = JSON.stringify({
      hook_event_name: 'PermissionRequest',
      tool_name: 'Bash',
      tool_input: {
        command: 'echo <img src=x onerror="window.__xss=1"> && <script>window.__xss=2</script>',
        description: '<svg onload="window.__xss=3"></svg><b>bold</b>',
      },
      permission_mode: 'default',
    });
    await writeFile(
      script,
      `#!/bin/bash\necho '${session}' | '${info.nh}' hook claude SessionStart\n` +
        `echo '${permission}' | '${info.nh}' hook claude PermissionRequest\n` +
        'exec -a claude /bin/sleep 3613\n',
      { mode: 0o755 },
    );
    await makeWorkspace('Xss', { command: { argv: [script] } });
    await page.getByRole('tab', { name: 'Inbox' }).click();
    const card = page.locator('.card-approval').filter({ hasText: 'window.__xss=1' });
    await expect(card).toHaveCount(1, { timeout: 20_000 });

    // The markup is text. It makes no element and no script ran.
    await expect(card.locator('img, script, svg[onload], b')).toHaveCount(0);
    await expect(card).toContainText('<img src=x onerror="window.__xss=1">');
    expect(
      await page.evaluate(() => (window as never as { __xss?: number }).__xss),
    ).toBeUndefined();

    // The drawer has no serious accessibility violation with the card in it.
    await page.addScriptTag({ content: axe.source });
    const result = await page.evaluate(async () => {
      const found = await (
        window as never as {
          axe: {
            run: (context: Element) => Promise<{ violations: AxeViolation[] }>;
          };
        }
      ).axe.run(document.querySelector('.drawer') as Element);
      return found.violations
        .filter((v) => v.impact === 'serious' || v.impact === 'critical')
        .map((v) => ({
          id: v.id,
          impact: v.impact,
          nodes: v.nodes.map((n) => n.target.join(' ')),
        }));
    });
    expect(result).toEqual([]);
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('the keys 1 to 4 answer an approval at a low risk and select a button at a high risk', async ({
  page,
  request,
}) => {
  const { info, makeWorkspace, calls } = await openShell(page, request);
  const dir = `${tmpdir()}/nh-ui-keys-${process.pid}`;
  const stop = async () => {
    await exec('pkill', ['-9', '-f', '^claude 363[01]$']).catch(() => undefined);
    await exec('pkill', ['-9', '-f', `${dir}/`]).catch(() => undefined);
  };
  /** A program that asks for a permission to run `command`, as Claude Code does with its hooks. */
  const asker = async (n: number, command: string) => {
    const session = JSON.stringify({ session_id: `s-keys-${n}`, cwd: '/work', source: 'startup' });
    const permission = JSON.stringify({
      hook_event_name: 'PermissionRequest',
      tool_name: 'Bash',
      tool_input: { command, description: command },
      permission_mode: 'default',
    });
    const script = `${dir}/claude${n}`;
    await writeFile(
      script,
      `#!/bin/bash\necho '${session}' | '${info.nh}' hook claude SessionStart\n` +
        `echo '${permission}' | '${info.nh}' hook claude PermissionRequest\n` +
        `exec -a claude /bin/sleep 363${n}\n`,
      { mode: 0o755 },
    );
    return script;
  };
  try {
    await mkdir(dir, { recursive: true });
    await makeWorkspace('Keys low', { command: { argv: [await asker(0, 'ls -la')] } });
    await makeWorkspace('Keys high', { command: { argv: [await asker(1, 'rm -rf build')] } });
    await page.getByRole('tab', { name: 'Inbox' }).click();
    const low = page.locator('.card-approval[data-state="open"]').filter({ hasText: 'ls -la' });
    const high = page
      .locator('.card-approval[data-state="open"]')
      .filter({ hasText: 'rm -rf build' });
    await expect(low).toHaveCount(1, { timeout: 20_000 });
    await expect(high).toHaveCount(1, { timeout: 20_000 });
    await expect(high.locator('.card-risk')).toHaveText('high risk');
    await expect(low.locator('.card-risk')).not.toHaveText('high risk');
    const answers = async () => (await calls('interaction.answer')).length;
    const before = await answers();

    // At a high risk, a number selects its button and gives it the focus; it does not answer.
    await high.focus();
    await page.keyboard.press('1');
    const allow = high.getByRole('button', { name: /Allow$/ });
    await expect(allow).toBeFocused();
    await expect(allow).toHaveAttribute('aria-pressed', 'true');
    expect(await answers()).toBe(before);
    await page.keyboard.press('3');
    await expect(high.getByRole('button', { name: /Deny$/ })).toBeFocused();
    // Enter on the selected button answers.
    await page.keyboard.press('Enter');
    await expect.poll(answers).toBe(before + 1);

    // At a low risk, a number answers at once. No rule covers `ls -la` alone, so the card has no
    // "Allow always", and 2 names nothing.
    await low.focus();
    await expect(low.getByRole('button', { name: /always/ })).toHaveCount(0);
    await page.keyboard.press('2');
    await page.keyboard.press('1');
    await expect.poll(answers).toBe(before + 2);
    const sent = (await calls('interaction.answer')).slice(-2).map((call) => call.params);
    expect(JSON.stringify(sent[0])).toContain('deny');
    expect(JSON.stringify(sent[1])).toContain('allow');
    expect(JSON.stringify(sent[1])).not.toContain('always');
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('the Files tab shows the tree of the folder of the tab, a file, and acts on a path', async ({
  page,
  request,
}) => {
  const { api, calls, makeWorkspace } = await openShell(page, request);
  const base = await realpath(await mkdtemp(join(tmpdir(), 'nh-files-')));
  try {
    await mkdir(join(base, 'src'));
    await writeFile(join(base, 'src', 'lib.rs'), 'pub fn answer() -> u32 {\n    42\n}\n');
    await writeFile(join(base, 'README.md'), '# Title\n\nSome *text*.\n');
    await writeFile(join(base, 'Zeta.txt'), 'z\n');
    await writeFile(join(base, '.env'), 'A=1\n');
    await mkdir(join(base, '.git'));
    // `cat` shows the paths that the page inserts.
    const made = await makeWorkspace('Files', {
      root: base,
      command: { argv: ['/bin/sh', '-c', 'stty -echo; exec cat'] },
    });
    await page.getByRole('tab', { name: 'Files' }).click();
    const tree = page.getByRole('tree', { name: 'Files' });
    const names = tree.locator('.files-row .files-name');
    await expect(page.locator('.files-root-path')).toHaveText(base);
    // The folders first, then the rest, by name without case. `.git` is not there.
    await expect(names).toHaveText(['src', '.env', 'README.md', 'Zeta.txt']);
    await expect(tree.locator('.files-row.hidden .files-name')).toHaveText(['.env']);
    expect((await calls('fs.list')).map((call) => call.params)).toEqual([{ path: base }]);
    // Each row has the icon that the core names for it, drawn when the art has loaded.
    await expect
      .poll(() =>
        tree
          .locator('.files-row .file-icon')
          .evaluateAll((icons) => icons.map((icon) => icon.getAttribute('data-icon'))),
      )
      .toEqual(['folder-src', 'tune', 'readme', 'document']);
    await expect(tree.locator('.files-row .file-icon path').first()).toBeAttached();

    // A click opens a folder, and the keyboard closes it again.
    const src = tree.locator(`[data-path="${join(base, 'src')}"]`);
    await src.click();
    await expect(src).toHaveAttribute('aria-expanded', 'true');
    await expect(names).toHaveText(['src', 'lib.rs', '.env', 'README.md', 'Zeta.txt']);
    await src.focus();
    await page.keyboard.press('ArrowLeft');
    await expect(src).toHaveAttribute('aria-expanded', 'false');
    await page.keyboard.press('ArrowRight');
    await expect(src).toHaveAttribute('aria-expanded', 'true');
    await page.keyboard.press('ArrowRight');
    const lib = tree.locator(`[data-path="${join(base, 'src', 'lib.rs')}"]`);
    await expect(lib).toBeFocused();

    // Enter opens the file under the tree: the lines with their numbers and highlight.
    await page.keyboard.press('Enter');
    const view = page.getByRole('region', { name: 'File' });
    await expect(view.locator('.file-name')).toHaveText('lib.rs');
    await expect(view.locator('.file-info')).toHaveText('3 lines · 34 bytes');
    await expect(view.locator('.file-num')).toHaveText(['1', '2', '3']);
    await expect(view.locator('.file-text').first()).toHaveText('pub fn answer() -> u32 {');
    await expect(view.locator('.file-text .hl-kw').first()).toHaveText('pub');
    await expect(lib).toHaveAttribute('aria-selected', 'true');

    // A Markdown file shows its HTML, and a switch shows its lines.
    await tree.locator(`[data-path="${join(base, 'README.md')}"]`).click();
    await expect(view.locator('.rich-html h1')).toHaveText('Title');
    await view.getByRole('button', { name: 'Source' }).click();
    await expect(view.locator('.file-text')).toHaveText(['# Title', '', 'Some *text*.']);

    // Open shows the same file in a large view over the window, and no app of the Mac. Escape
    // closes it and gives the focus back. WebKit gives no focus to a clicked button, so the row of
    // the file keeps it.
    const openButton = view.getByRole('button', { name: 'Open', exact: true });
    await openButton.click();
    const large = page.getByRole('dialog', { name: 'README.md' });
    await expect(large).toBeVisible();
    await expect(large.locator('.file-modal-path')).toHaveText(join(base, 'README.md'));
    await expect(large.locator('.file-icon')).toHaveAttribute('data-icon', 'readme');
    await expect(large.locator('.rich-html h1')).toHaveText('Title');
    await expect(large.locator('.file-body')).toBeFocused();
    await expect(
      large.getByRole('toolbar', { name: 'File actions' }).getByRole('button'),
    ).toHaveText(['Open in app', 'Reveal', 'Copy', 'Copy relative', 'Insert', 'Source']);
    expect(await calls('fs.open')).toEqual([]);
    await page.keyboard.press('Escape');
    await expect(large).toHaveCount(0);
    await expect(tree.locator(`[data-path="${join(base, 'README.md')}"]`)).toBeFocused();

    // With no pane in focus, Insert says so. Then it pastes the quoted path into the pane that
    // has the focus, and gives the keyboard to that pane.
    await view.getByRole('button', { name: 'Insert into pane' }).click();
    await expect(page.locator('.files-message')).toHaveText(
      'No pane has the focus. Click a pane first.',
    );
    await page.locator(`[data-pane="${made.pane}"] .pane-surface`).click();
    await view.getByRole('button', { name: 'Insert into pane' }).click();
    await expect(page.locator('.files-message')).toHaveCount(0);
    await expect(page.locator(`[data-pane="${made.pane}"] .pane-input`)).toBeFocused();
    await expect
      .poll(async () => (await calls('pane.insert_paths')).map((call) => call.params['paths']))
      .toEqual([[join(base, 'README.md')]]);
    expect((await calls('pane.insert_paths'))[0]?.params['pane']).toBe(made.pane);
    await api('pane.send_bytes', { pane: made.pane, input_id: 1, bytes: 'Cg==' });
    await expect
      // A long path wraps at the edge of the pane.
      .poll(async () => (await screenOf(api, made.pane)).replaceAll('\n', ''))
      .toContain(`'${join(base, 'README.md')}'`);

    // A drag of a row onto a pane inserts its path. In the app the host takes the drag events of
    // HTML, so the page stops them here too, and the drag works with pointer events.
    await page.evaluate(() => {
      for (const type of ['dragstart', 'dragenter', 'dragover', 'dragleave', 'drop']) {
        window.addEventListener(type, (event) => event.stopImmediatePropagation(), true);
      }
    });
    await tree
      .locator(`[data-path="${join(base, 'Zeta.txt')}"]`)
      .dragTo(page.locator(`[data-pane="${made.pane}"] .pane-surface`));
    await expect
      .poll(async () => (await calls('pane.insert_paths')).map((call) => call.params['paths']))
      .toEqual([[join(base, 'README.md')], [join(base, 'Zeta.txt')]]);
    // The drag opened no file: the click after it does not reach the row.
    await expect(view.locator('.file-name')).toHaveText('README.md');

    // The menu of a row has the actions, and Escape closes it.
    await tree.locator(`[data-path="${join(base, 'Zeta.txt')}"]`).click({ button: 'right' });
    const menu = page.getByRole('menu', { name: 'Actions of Zeta.txt' });
    await expect(menu.getByRole('menuitem')).toHaveText([
      'Open',
      'Open in app',
      'Reveal in Finder',
      'Copy path',
      'Copy relative path',
      'Insert into pane',
    ]);
    await expect(menu.getByRole('menuitem').first()).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(menu).toHaveCount(0);
    // Shift+F10 on a row opens its menu too, and Escape gives the focus back to the row.
    const zeta = tree.locator(`[data-path="${join(base, 'Zeta.txt')}"]`);
    await zeta.focus();
    await page.keyboard.press('Shift+F10');
    await expect(menu.getByRole('menuitem').first()).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(menu).toHaveCount(0);
    await expect(zeta).toBeFocused();
    await tree.locator(`[data-path="${join(base, 'Zeta.txt')}"]`).click({ button: 'right' });
    await menu.getByRole('menuitem', { name: 'Copy relative path' }).click();
    // The core of this test has no host, so the page shows that nothing took the text. Through
    // `link.sock` the link client is the host, and it takes the text. The clipboard of a host is
    // in `crates/nh/tests/files.rs`.
    await expect(page.locator('.files-message')).toHaveText(
      process.env['NEOHAR_UI_LINK'] === '1'
        ? 'Copied the relative path.'
        : 'This client has no clipboard that the core can write.',
    );
    expect((await calls('fs.copy_path')).map((call) => call.params)).toEqual([
      { path: join(base, 'Zeta.txt'), relative_to: base },
    ]);

    // The drawer with a file open has no serious accessibility violation.
    await page.addScriptTag({ content: axe.source });
    const violations = await page.evaluate(async () => {
      const found = await (
        window as never as {
          axe: { run: (context: Element) => Promise<{ violations: AxeViolation[] }> };
        }
      ).axe.run(document.querySelector('.drawer') as Element);
      return found.violations
        .filter((v) => v.impact === 'serious' || v.impact === 'critical')
        .map((v) => ({ id: v.id, nodes: v.nodes.map((n) => n.target.join(' ')) }));
    });
    expect(violations).toEqual([]);

    // Open in the menu of a file reads it and shows it in the large view, which has no serious
    // accessibility violation either. Insert there closes the view and gives the pane the keyboard.
    await zeta.click({ button: 'right' });
    await menu.getByRole('menuitem', { name: 'Open', exact: true }).click();
    const zetaView = page.getByRole('dialog', { name: 'Zeta.txt' });
    await expect(zetaView.locator('.file-text')).toHaveText(['z']);
    // The file came after the view opened, and its text took the focus then.
    await expect(zetaView.locator('.file-body')).toBeFocused();
    await expect(view.locator('.file-name')).toHaveText('Zeta.txt');
    const largeViolations = await page.evaluate(async () => {
      const found = await (
        window as never as {
          axe: { run: (context: Element) => Promise<{ violations: AxeViolation[] }> };
        }
      ).axe.run(document.querySelector('.file-modal') as Element);
      return found.violations
        .filter((v) => v.impact === 'serious' || v.impact === 'critical')
        .map((v) => ({ id: v.id, nodes: v.nodes.map((n) => n.target.join(' ')) }));
    });
    expect(largeViolations).toEqual([]);
    await zetaView.getByRole('button', { name: 'Insert into pane' }).click();
    await expect(zetaView).toHaveCount(0);
    await expect(page.locator(`[data-pane="${made.pane}"] .pane-input`)).toBeFocused();
    await expect
      .poll(async () => (await calls('pane.insert_paths')).map((call) => call.params['paths']))
      .toEqual([[join(base, 'README.md')], [join(base, 'Zeta.txt')], [join(base, 'Zeta.txt')]]);

    // Refresh lists the open folders again and shows a new file with no other action.
    await writeFile(join(base, 'src', 'main.rs'), 'fn main() {}\n');
    await page.getByRole('button', { name: 'Refresh' }).click();
    await expect(names).toHaveText(['src', 'lib.rs', 'main.rs', '.env', 'README.md', 'Zeta.txt']);

    // The root follows the folder where the pane with the focus is now. Before the pane
    // has the focus, the root is the root of the workspace.
    const follow = await makeWorkspace('Follow', {
      root: base,
      command: {
        argv: ['/bin/sh', '-c', 'while [ ! -e go ]; do sleep 0.05; done; cd src && exec sleep 60'],
      },
    });
    await expect(page.locator('.files-root-path')).toHaveText(base);
    await page.locator(`[data-pane="${follow.pane}"] .pane-surface`).click();
    await writeFile(join(base, 'go'), '');
    await expect(page.locator('.files-root-path')).toHaveText(join(base, 'src'));
    await expect(names).toHaveText(['lib.rs', 'main.rs']);

    // A tab without a folder says so. The pane with the focus is in another tab, so its folder
    // does not count.
    await makeWorkspace('No folder');
    await expect(page.locator('.files-panel')).toHaveText('This tab has no folder.');
  } finally {
    await rm(base, { recursive: true, force: true });
  }
});

test('the Changes tab shows the changes of the worktree of the folder, and the diff of a row', async ({
  page,
  request,
}) => {
  const { calls, makeWorkspace, windowId } = await openShell(page, request);
  const base = await realpath(await mkdtemp(join(tmpdir(), 'nh-changes-')));
  const repo = join(base, 'app');
  const git = (cwd: string, ...args: string[]) =>
    exec('git', args, {
      cwd,
      env: {
        ...process.env,
        GIT_CONFIG_GLOBAL: '/dev/null',
        GIT_AUTHOR_NAME: 'Ada',
        GIT_AUTHOR_EMAIL: 'ada@example.com',
        GIT_COMMITTER_NAME: 'Ada',
        GIT_COMMITTER_EMAIL: 'ada@example.com',
      },
    });
  try {
    await mkdir(repo, { recursive: true });
    await git(repo, 'init', '-q', '-b', 'main');
    await writeFile(join(repo, 'a.txt'), 'one\ntwo\n');
    await git(repo, 'add', '.');
    await git(repo, 'commit', '-q', '-m', 'first');

    // A tab without a folder says so. The pane with the focus is in another tab, so its folder
    // does not count, as in the Files tab.
    await makeWorkspace('Plain');
    await page.getByRole('tab', { name: 'Changes' }).click();
    const panel = page.locator('.changes-panel');
    await expect(panel).toHaveText('This tab has no folder.');

    // A workspace whose root is the repository. Its pane goes out of the repository when the
    // file `go` is there.
    const checkout = await makeWorkspace('Checkout', {
      root: repo,
      command: {
        argv: [
          '/bin/sh',
          '-c',
          `while [ ! -e '${base}/go' ]; do sleep 0.05; done; cd '${base}' && exec sleep 60`,
        ],
      },
    });
    // The core runs no git in a repository that the person did not trust: the tab asks first.
    const prompt = panel.getByTestId('trust-prompt');
    await expect(prompt).toContainText('Do you trust this repository?');
    await expect(prompt.locator('.trust-path')).toHaveText(repo);
    await prompt.getByRole('button', { name: 'Trust' }).click();
    await expect(prompt).toHaveCount(0);
    await expect(panel.locator('.changes-branch-name')).toHaveText('main');
    await expect(panel.locator('.changes-root')).toHaveText(repo);
    await expect(panel).toContainText('No changes yet.');

    // Files that change while the tab shows: the rows come with no action of the page. The shell
    // test runs `nh daemon`, which watches only the git directory, so `git add` is the cause. The
    // changes go from HEAD, so a file in the index and a file that git does not track are rows.
    await writeFile(join(repo, 'b.txt'), 'new\n');
    await writeFile(join(repo, 'a.txt'), 'one\ntwo\nthree\n');
    await writeFile(join(repo, 'loose.txt'), 'x\n');
    await git(repo, 'add', 'a.txt', 'b.txt');
    const rows = panel.locator('.changes-row');
    await expect(rows).toHaveCount(3, { timeout: 15_000 });
    // The order of the core, the status as one letter and the lines.
    await expect(rows.locator('.changes-name')).toHaveText(['a.txt', 'b.txt', 'loose.txt']);
    await expect(rows.locator('.changes-status [aria-hidden]')).toHaveText(['M', 'A', 'U']);
    await expect(rows.first().locator('.changes-lines')).toHaveText('+1 −0');
    await expect(panel.getByTestId('changes-facts')).toHaveText('3 files · +3 −0');
    expect(await calls('changes.diff')).toEqual([]);

    // A click opens the diff of the row under it, from the core.
    const a = panel.locator('[data-path="a.txt"]');
    await a.click();
    await expect(a).toHaveAttribute('aria-expanded', 'true');
    const diffOfA = panel.getByRole('group', { name: 'Diff of a.txt' });
    await expect(diffOfA.locator('.diff-add .diff-text')).toHaveText(['three']);
    expect((await calls('changes.diff')).map((call) => call.params)).toEqual([
      { window: windowId, path: 'a.txt' },
    ]);

    // The keys move between the rows, and Enter opens a second diff.
    await expect(a).toBeFocused();
    await page.keyboard.press('ArrowDown');
    const b = panel.locator('[data-path="b.txt"]');
    await expect(b).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(b).toHaveAttribute('aria-expanded', 'true');
    await expect(
      panel.getByRole('group', { name: 'Diff of b.txt' }).locator('.diff-add .diff-text'),
    ).toHaveText(['new']);

    // A new stamp of an open row loads its diff again.
    await writeFile(join(repo, 'a.txt'), 'one\ntwo\nthree\nfour\n');
    await git(repo, 'add', 'a.txt');
    await expect(diffOfA.locator('.diff-add .diff-text')).toHaveText(['three', 'four'], {
      timeout: 15_000,
    });
    expect(
      (await calls('changes.diff')).filter((call) => call.params.path === 'a.txt'),
    ).toHaveLength(2);

    // A second click closes the diff. The drawer has no serious accessibility violation.
    await a.click();
    await expect(a).toHaveAttribute('aria-expanded', 'false');
    await expect(diffOfA).toHaveCount(0);
    await page.addScriptTag({ content: axe.source });
    const violations = await page.evaluate(async () => {
      const found = await (
        window as never as {
          axe: { run: (context: Element) => Promise<{ violations: AxeViolation[] }> };
        }
      ).axe.run(document.querySelector('.drawer') as Element);
      return found.violations
        .filter((v) => v.impact === 'serious' || v.impact === 'critical')
        .map((v) => ({ id: v.id, nodes: v.nodes.map((n) => n.target.join(' ')) }));
    });
    expect(violations).toEqual([]);

    // The folder follows the pane with the focus: a `cd` out of the repository says so.
    await page.locator(`[data-pane="${checkout.pane}"] .pane-surface`).click();
    await writeFile(join(base, 'go'), '');
    await expect(panel).toHaveText('This folder is not in a git repository.');
  } finally {
    await rm(base, { recursive: true, force: true });
  }
});

test('the Workflows view builds a workflow on the canvas and shows its execution', async ({
  page,
  request,
}) => {
  const { makeWorkspace } = await openShell(page, request);
  const project = await realpath(await mkdtemp(join(tmpdir(), 'nh-wf-')));
  await makeWorkspace('Flows', { root: project });
  const call = <T>(method: string, params: Record<string, unknown>) =>
    page.evaluate(([m, p]) => (window as never as NhTest).nhTest.call(m as string, p), [
      method,
      params,
    ] as const) as Promise<T>;
  // The switch of the status bar opens the view over the tab bar and the panes.
  await page.locator('.status-views button', { hasText: 'Workflows' }).click();
  await expect(page.locator('.wf-view')).toBeVisible();
  await expect(page.locator('.tabbar')).toHaveCount(0);
  // A new workflow from the preset Review has the trigger, three reviews and the report.
  await page.locator('.wf-side-actions button', { hasText: '+ New' }).click();
  await page.locator('.wf-new input[name=name]').fill('Review');
  await page.locator('.wf-new select[name=preset]').selectOption('review');
  await page.locator('.wf-new button[type=submit]').click();
  await expect(page.locator('.wf-node')).toHaveCount(5);
  await expect(page.locator('.wf-edges .wf-edge')).toHaveCount(6);

  // A drag from the output port of the report to an empty place opens the picker there.
  const port = page.locator('.wf-node[data-node="report"] .wf-port.out');
  const box = await port.boundingBox();
  if (!box) throw new Error('the port has no box');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 160, box.y + 140, { steps: 8 });
  await page.mouse.up();
  await expect(page.locator('.wf-picker')).toBeVisible();
  await page.locator('.wf-picker-filter').fill('codex agent');
  await page.keyboard.press('Enter');
  await expect(page.locator('.wf-node')).toHaveCount(6);
  const saved = await call<{ graph: { nodes: { id: string; inputs?: string[] }[] } }>(
    'workflow.show',
    { workflow: 'review' },
  );
  expect(saved.graph.nodes.find((node) => node.id === 'codex')?.inputs).toEqual(['report']);

  // Delete removes the selected node and its connection.
  await page.locator('.wf-node[data-node="codex"] .wf-node-title').click();
  await page.keyboard.press('Delete');
  await expect(page.locator('.wf-node')).toHaveCount(5);
  await expect(page.locator('.wf-edges .wf-edge')).toHaveCount(6);

  // A double-click opens the details: inputs, settings and output.
  await page.locator('.wf-node[data-node="logic"] .wf-node-title').dblclick();
  await expect(page.locator('.wf-details')).toBeVisible();
  await expect(page.locator('.wf-details .wf-chip')).toHaveCount(5);
  await page.locator('.wf-details .dialog-cancel').click();
  await expect(page.locator('.wf-details')).toHaveCount(0);

  // An execution shows its nodes with their states, and a panel with the execution.
  await page.locator('.wf-execute-button').click();
  await expect(page.locator('.wf-execute')).toBeVisible();
  await page.locator('.wf-execute button[type=submit]').click();
  await expect(page.locator('.wf-panel')).toBeVisible();
  await expect(page.locator('.wf-execution')).toHaveCount(1);
  await expect(page.locator('.wf-node[data-node="start"] .wf-node-state')).toContainText('done');
  await page.locator('.wf-panel-head button', { hasText: 'Cancel' }).click();
  await expect(page.locator('.wf-panel-title')).toContainText('cancelled');

  // Escape closes the view, and the tab bar is back.
  await page.locator('.wf-canvas').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await expect(page.locator('.wf-view')).toHaveCount(0);
  await expect(page.locator('.tabbar')).toBeVisible();
  await rm(project, { recursive: true, force: true });
});
