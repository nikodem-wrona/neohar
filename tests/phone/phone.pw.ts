// The phone app in WebKit against a headless core: an iPhone 17 Pro Max
// as the app of the home screen shows it, and an iPhone 15 for one test of the narrow width. The
// listener of the phone serves the page on `127.0.0.1`. A spec pairs the page, and the paired
// phone shows the desk with the form `phone`. The CLI and small programs in panes make what the
// phone shows: a question of an agent, a run whose turn ended, a long message of the agent.
//
// The programs tell the core about their session through the hooks of Claude Code (`nh hook
// claude …`), and then stay alive under the name `claude`. The core has no window, so no pane has
// the focus, and a turn that ends gets the reason `turn_ended` after 3 s.
//
// `NEOHAR_PHONE_SHOTS=<folder>` keeps pictures of the desk in portrait and in landscape.

import { execFile } from 'node:child_process';
import { mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { type APIRequestContext, expect, type Page, test } from '@playwright/test';
import axe from 'axe-core';
import { decodePng, inkPixels } from '../ui/png';

const exec = promisify(execFile);

interface Info {
  api_socket: string;
  nh: string;
  phone_port: number;
  /** The dev WebSocket of the core, with the scope of the UI: a window of a desk for a test. */
  ws_port: number;
  token: string;
}

interface AxeViolation {
  id: string;
  impact: string | null;
  nodes: { target: unknown[] }[];
}

const shots = process.env.NEOHAR_PHONE_SHOTS;

/** The core of the test server: where its listener is, and a call on the socket of the CLI. */
async function core(request: APIRequestContext) {
  const info = (await (await request.get('/info')).json()) as Info;
  const origin = `http://127.0.0.1:${info.phone_port}`;
  const api = async <T = Record<string, unknown>>(
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
      const stderr = (failure as { stderr?: string }).stderr ?? '';
      throw new Error(`${method}: ${stderr.trim() || String(failure)}`, { cause: failure });
    }
  };
  return { info, origin, api };
}

type Core = Awaited<ReturnType<typeof core>>;

/** Pairs the browser of the page with a new offer, as the app of the home screen does. */
async function pair(page: Page, { origin, api }: Core): Promise<void> {
  const offer = await api<{ code: string }>('phone.pair_begin', {});
  const answer = await page.request.post(`${origin}/api/pair`, {
    headers: { Origin: origin },
    data: { code: offer.code, name: 'Test iPhone' },
  });
  expect(answer.status()).toBe(200);
  await page.goto(`${origin}/`);
  await desk(page);
}

/** The desk of a paired phone shows: the shell with the form `phone` and its bottom bar. */
async function desk(page: Page): Promise<void> {
  await expect(page.locator('.shell.phone')).toBeVisible({ timeout: 20_000 });
}

/** The button of the bottom bar for a tab of the drawer, or More. */
function dock(page: Page, tab: string) {
  return page.locator(`.phone-dock [data-tab="${tab}"]`);
}

/** Opens the settings of the phone at a section: More, then Settings, then the section. */
async function settings(page: Page, section: string): Promise<void> {
  await dock(page, 'more').click();
  await page
    .getByRole('dialog', { name: 'More' })
    .getByRole('button', { name: 'Settings' })
    .click();
  const sheet = page.getByRole('dialog', { name: 'Settings' });
  await sheet.locator(`[data-section="${section}"]`).click();
  await expect(sheet.locator('h1.settings-title')).toHaveText(new RegExp(section, 'i'));
}

/** The pixels that a locator shows with a colour other than its background. */
async function ink(page: Page, selector: string): Promise<number> {
  return inkPixels(decodePng(await page.locator(selector).screenshot()));
}

/** A run of the core by its pane, from `agent.list`. */
async function runOf(c: Core, pane: string): Promise<string> {
  let found: string | undefined;
  await expect
    .poll(
      async () => {
        const list = await c.api<{ agents: { id: string; pane?: string | null }[] }>(
          'agent.list',
          {},
        );
        found = list.agents.find((agent) => agent.pane === pane)?.id;
        return found;
      },
      { timeout: 30_000 },
    )
    .toBeTruthy();
  return found ?? '';
}

interface Hook {
  event: string;
  payload: Record<string, unknown>;
}

/**
 * A program for a pane: it sends the hooks, and then runs `tail` as `claude`. The payloads go
 * through files, so that no shell quoting touches them.
 */
async function program(dir: string, nh: string, hooks: Hook[], tail: string): Promise<string> {
  await mkdir(dir, { recursive: true });
  const lines = ['#!/bin/bash'];
  for (const [at, hook] of hooks.entries()) {
    const file = join(dir, `${at}.json`);
    await writeFile(file, JSON.stringify(hook.payload));
    lines.push(`'${nh}' hook claude ${hook.event} < '${file}'`);
  }
  lines.push(`exec -a claude ${tail}`);
  const script = join(dir, 'claude');
  await writeFile(script, `${lines.join('\n')}\n`, { mode: 0o755 });
  return script;
}

async function violations(page: Page, selector: string): Promise<unknown[]> {
  // The policy of the page refuses a script element, so axe comes in through the browser.
  await page.evaluate(axe.source);
  return page.evaluate(async (target) => {
    const found = await (
      window as never as {
        axe: { run: (context: Element) => Promise<{ violations: AxeViolation[] }> };
      }
    ).axe.run(document.querySelector(target) as Element);
    return found.violations
      .filter((v) => v.impact === 'serious' || v.impact === 'critical')
      .map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.map((n) => n.target.join(' ')) }));
  }, selector);
}

/** The page is no wider than the screen. */
async function noSideScroll(page: Page): Promise<void> {
  const widths = await page.evaluate(() => ({
    page: document.documentElement.scrollWidth,
    screen: window.innerWidth,
  }));
  expect(widths.page).toBeLessThanOrEqual(widths.screen);
}

test('a phone pairs with the short code, and then shows the desk', async ({ page, request }) => {
  const c = await core(request);
  const { origin, api } = c;

  // The page of the QR code names the code in its link to the manifest, and the manifest starts
  // the app of the home screen with it.
  const offer = await api<{ code: string; url: string }>('phone.pair_begin', {});
  const long = offer.url.split('code=')[1] ?? '';
  const shown = await (await request.get(`${origin}/pair?code=${long}`)).text();
  expect(shown).toContain(`href="/manifest.webmanifest?code=${long}"`);
  const manifest = (await (
    await request.get(`${origin}/manifest.webmanifest?code=${long}`)
  ).json()) as { start_url: string; display: string; icons: { src: string }[] };
  expect(manifest.start_url).toBe(`/?code=${long}`);
  expect(manifest.display).toBe('standalone');
  expect(manifest.icons.map((icon) => icon.src)).toContain('/icon-192.png');
  // The colours of the theme come from the listener.
  expect(await (await request.get(`${origin}/theme.css`)).text()).toContain('--bg: #000000;');

  // In the browser, the page tells the person to add the app to the home screen, and it does not
  // spend the code: the app of the home screen has its own cookies.
  await page.goto(offer.url);
  await expect(page.getByRole('heading', { name: 'Pair this phone with the Mac' })).toBeVisible();
  await expect(page.getByText('Add to Home Screen')).toBeVisible();
  await expect(page.getByText('It pairs by itself.')).toBeVisible();

  // A wrong code gets the one answer of every refusal.
  const field = page.getByLabel('The code that the Mac shows');
  await field.fill('WRONG234');
  await page.getByRole('button', { name: 'Pair' }).click();
  await expect(page.getByRole('alert')).toContainText('The code is wrong, or the offer ended.');

  await field.fill(offer.code);
  await page.getByRole('button', { name: 'Pair' }).click();
  // The paired phone shows the desk, with the bottom bar of the drawer.
  await desk(page);
  await expect(page.getByRole('navigation', { name: 'Drawer' })).toBeVisible();
  await expect(page.locator('.statusbar')).toBeHidden();
  // The address keeps no copy of the code.
  expect(new URL(page.url()).search).toBe('');

  // The phone knows its name from the Mac: This phone is in the section Phone of the settings.
  await settings(page, 'phone');
  const sheet = page.getByRole('dialog', { name: 'Settings' });
  expect(new URL(page.url()).hash).toBe('#/settings/phone');
  const own = sheet.getByRole('region', { name: 'This phone' });
  await expect(own.getByRole('textbox', { name: 'Name' })).toHaveValue('Phone');
  await own.getByRole('textbox', { name: 'Name' }).fill('Test iPhone');
  await own.getByRole('button', { name: 'Rename' }).click();
  await expect(own.getByRole('textbox', { name: 'Name' })).toHaveValue('Test iPhone');
  await expect(own.getByRole('button', { name: 'Rename' })).toBeDisabled();
  // Close takes the address of the sheet with it.
  await sheet.getByRole('button', { name: 'Close' }).click();
  await expect(sheet).toHaveCount(0);
  expect(new URL(page.url()).hash).toBe('#/');
});

test('the app of the home screen pairs by itself with the code of its address', async ({
  page,
  request,
}) => {
  const { origin, api } = await core(request);
  // Safari says so for an app that runs from the home screen.
  await page.addInitScript(() => Object.defineProperty(navigator, 'standalone', { value: true }));
  const offer = await api<{ url: string }>('phone.pair_begin', {});
  const long = offer.url.split('code=')[1] ?? '';
  // The `start_url` of the manifest.
  await page.goto(`${origin}/?code=${long}`);
  await desk(page);
  await expect(page.getByText('Add to Home Screen')).toHaveCount(0);
  expect(new URL(page.url()).search).toBe('');
  // The code is spent: a second start with it gets the refusal.
  await page.context().clearCookies();
  await page.goto(`${origin}/?code=${long}`);
  await expect(page.getByRole('alert')).toContainText('The code is wrong, or the offer ended.');
});

/** The hooks of a question of Claude Code, for `program`. */
function question(session: string, dir: string, text: string): Hook[] {
  return [
    { event: 'SessionStart', payload: { session_id: session, cwd: dir, source: 'startup' } },
    {
      event: 'PreToolUse',
      payload: {
        session_id: session,
        hook_event_name: 'PreToolUse',
        tool_name: 'AskUserQuestion',
        tool_use_id: `toolu_${session}`,
        tool_input: {
          questions: [
            {
              header: 'Color',
              question: text,
              multiSelect: false,
              options: [
                { label: 'Red', description: 'A warm color' },
                { label: 'Blue', description: 'A cool color' },
              ],
            },
          ],
        },
      },
    },
  ];
}

test('a card of a question answers with via phone', async ({ page, request }) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-question-${process.pid}`);
  const stop = () => exec('pkill', ['-9', '-f', '^claude 3701$']).catch(() => undefined);
  try {
    const script = await program(
      dir,
      c.info.nh,
      question('s-phone-question', dir, 'Which color do you want?'),
      '/bin/sleep 3701',
    );
    await c.api('workspace.create', { name: 'Question', command: { argv: [script] } });

    // The Inbox of the bottom bar opens the drawer as a sheet with the card.
    await expect(dock(page, 'inbox')).toHaveClass(/hot/, { timeout: 30_000 });
    await dock(page, 'inbox').click();
    const card = page
      .locator('.drawer article.card-question')
      .filter({ hasText: 'Which color do you want?' });
    await expect(card).toHaveCount(1, { timeout: 30_000 });
    const id = await card.getAttribute('data-interaction');
    expect(id).toBeTruthy();
    await card.getByRole('radio', { name: /Blue/ }).check();
    await card.getByRole('button', { name: 'Answer' }).click();

    // The delivery record names the phone as the way of the answer.
    await expect
      .poll(
        async () =>
          (
            await c.api<{ interaction: { delivery: { via?: string } } }>('interaction.show', {
              interaction: id,
            })
          ).interaction.delivery.via,
        { timeout: 20_000 },
      )
      .toBe('phone');
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('a push of a question opens the Inbox with its card', async ({ page, request }) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-push-${process.pid}`);
  const stop = () => exec('pkill', ['-9', '-f', '^claude 3702$']).catch(() => undefined);
  try {
    const script = await program(
      dir,
      c.info.nh,
      question('s-phone-push', dir, 'Which color for the push?'),
      '/bin/sleep 3702',
    );
    const made = await c.api<{ pane: string }>('workspace.create', {
      name: 'Push',
      command: { argv: [script] },
    });
    const run = await runOf(c, made.pane);
    await expect
      .poll(async () => {
        const list = await c.api<{ interactions: { run?: string }[] }>('interaction.list', {});
        return list.interactions.some((row) => row.run === run);
      })
      .toBe(true);
    // The tap on the push opens `#/run/<id>`: the Inbox shows the card of the run.
    await page.goto(`${c.origin}/#/run/${run}`);
    await desk(page);
    await expect(
      page
        .locator('.drawer article.card-question')
        .filter({ hasText: 'Which color for the push?' }),
    ).toBeVisible({ timeout: 20_000 });
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('a run whose turn ended takes a reply in the Inbox and shows its last message in full', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-reply-${process.pid}`);
  const transcript = join(dir, 'session.jsonl');
  const log = join(dir, 'input.log');
  const stop = () => exec('pkill', ['-9', '-f', '^claude -u -c']).catch(() => undefined);
  try {
    await mkdir(dir, { recursive: true });
    await writeFile(log, '');
    // The message of the agent is longer than the preview of 300 characters of the timeline.
    const words = Array.from({ length: 60 }, (_, at) => `word${at}`).join(' ');
    const at = () => new Date().toISOString();
    const line = (value: unknown) => `${JSON.stringify(value)}\n`;
    await writeFile(
      transcript,
      line({ type: 'user', uuid: 'u1', timestamp: at(), message: { content: 'write a summary' } }) +
        line({
          type: 'assistant',
          uuid: 'a1',
          timestamp: at(),
          message: {
            id: 'm1',
            model: 'claude-opus-5-5',
            content: [
              { type: 'text', text: `## Summary\n\n${words}\n\nThe **last** line: END-OF-MESSAGE` },
            ],
            usage: { input_tokens: 100, output_tokens: 200 },
          },
        }),
    );
    const session = 's-phone-reply';
    const script = await program(
      dir,
      c.info.nh,
      [
        {
          event: 'SessionStart',
          payload: {
            session_id: session,
            cwd: dir,
            source: 'startup',
            transcript_path: transcript,
            model: 'claude-opus-5-5',
          },
        },
        {
          event: 'UserPromptSubmit',
          payload: { session_id: session, prompt: 'write a summary', transcript_path: transcript },
        },
        { event: 'Stop', payload: { session_id: session, transcript_path: transcript } },
      ],
      // The program writes each line of its input to the log.
      `/usr/bin/python3 -u -c 'import sys
log = open(sys.argv[1], "a")
for line in sys.stdin:
    log.write(line)
    log.flush()' '${log}'`,
    );
    const made = await c.api<{ pane: string }>('workspace.create', {
      name: 'Summary',
      command: { argv: [script] },
    });

    // The turn ended while nobody looked: the Inbox has a line for the run, with the box Reply.
    await dock(page, 'inbox').click();
    const ended = page.locator('.card-run-state[data-state="turn_ended"]');
    await expect(ended).toHaveCount(1, { timeout: 30_000 });
    await expect(ended).toContainText('finished its turn');
    await noSideScroll(page);
    expect(await violations(page, '.shell'), 'the Inbox').toEqual([]);
    if (shots) await page.screenshot({ path: join(shots, 'phone-inbox.png') });

    // The reply goes to the program once, as a paste and Enter.
    const reply = ended.getByRole('form', { name: 'Reply' });
    await reply.getByRole('textbox').fill('next step please');
    await reply.getByRole('button', { name: 'Send' }).click();
    await expect
      .poll(async () => await readFile(log, 'utf8'), { timeout: 20_000 })
      .toContain('next step please');
    expect((await readFile(log, 'utf8')).match(/next step please/g)).toHaveLength(1);
    // The reply ended the reason, so the Inbox has no line for the run.
    await expect(ended).toHaveCount(0, { timeout: 20_000 });

    // The pane of the run, and the Timeline with the last message in full from `timeline.item`.
    await page.goto(`${c.origin}/#/pane/${made.pane}`);
    await desk(page);
    await expect(page.locator(`.layout [data-pane="${made.pane}"]`)).toBeVisible({
      timeout: 20_000,
    });
    await dock(page, 'timeline').click();
    const message = page.getByRole('region', { name: 'Last message' });
    await expect(message).toContainText('END-OF-MESSAGE', { timeout: 20_000 });
    await expect(message.locator('h2')).toHaveText('Summary');
    await expect(message.locator('strong')).toHaveText('last');
    await noSideScroll(page);
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('the desk paints one pane in portrait, with a dot for each pane of the tab', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const lines = `i=0; while [ $i -lt 10 ]; do echo "################################"; i=$((i+1)); done`;
  const made = await c.api<{ pane: string; tab: string }>('workspace.create', {
    name: 'Paint',
    command: { argv: ['/bin/sh', '-c', `${lines}; exec sleep 600`] },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  const surface = `[data-pane="${made.pane}"] .pane-surface`;
  await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
  // The renderer comes after the first paint, and then the pane paints.
  await expect.poll(() => ink(page, surface), { timeout: 20_000 }).toBeGreaterThan(2000);

  // A second pane of the tab: the single view still draws one pane, with two dots.
  const split = await c.api<{ pane: string }>('pane.split', {
    pane: made.pane,
    direction: 'right',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  await expect(page.locator('.pane-dots .pane-dot')).toHaveCount(2, { timeout: 20_000 });
  await expect(page.locator('.layout .pane')).toHaveCount(1);
  // The other pane gets the focus, and the core draws it in place of the first one.
  await page.goto(`${c.origin}/#/pane/${split.pane}`);
  await expect(page.locator(`.layout [data-pane="${split.pane}"]`)).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.locator('.layout .pane')).toHaveCount(1);
  await expect(page.locator('.pane-dot.current')).toHaveCount(1);
  if (shots) await page.screenshot({ path: join(shots, 'phone-portrait.png') });

  // Landscape has the layout of the desk with both columns as rails, and both panes.
  await page.setViewportSize({ width: 956, height: 440 });
  await expect(page.locator('.layout .pane')).toHaveCount(2, { timeout: 20_000 });
  await expect(page.locator('.statusbar')).toBeVisible();
  await expect(page.locator('.phone-dock')).toBeHidden();
  await expect(page.locator('.left-rail')).toBeVisible();
  await expect(page.locator('.drawer-rail')).toBeVisible();
  // A finger near the divider takes it: the divider is wider for a touch than it looks.
  const divider = await page.locator('.layout .divider').first().boundingBox();
  if (!divider) throw new Error('no divider in landscape');
  const near = { x: divider.x - 6, y: divider.y + divider.height / 2 };
  expect(
    await page.evaluate(
      ({ x, y }) => document.elementFromPoint(x, y)?.closest('.divider') !== null,
      near,
    ),
  ).toBe(true);
  await noSideScroll(page);
  expect(await violations(page, '.shell'), 'landscape').toEqual([]);
  if (shots) await page.screenshot({ path: join(shots, 'phone-landscape.png') });
  await page.setViewportSize({ width: 440, height: 956 });
  await expect(page.locator('.layout .pane')).toHaveCount(1, { timeout: 20_000 });
});

test('the drawer, the left column and More are sheets, and the palette has the width of the screen', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const made = await c.api<{ pane: string; tab: string }>('workspace.create', {
    name: 'Sheets',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  const main = await page.locator('.main').boundingBox();
  expect(main).not.toBeNull();

  // The drawer covers the pane area, and a second tap closes it.
  await dock(page, 'files').click();
  const drawer = page.getByRole('complementary', { name: 'Drawer' });
  await expect(drawer).toBeVisible();
  const box = await drawer.boundingBox();
  expect(Math.abs((box?.width ?? 0) - (main?.width ?? 0))).toBeLessThan(2);
  expect(Math.abs((box?.y ?? 0) - (main?.y ?? 0))).toBeLessThan(2);
  await expect(drawer.getByRole('tab', { selected: true })).toHaveText('Files');
  await dock(page, 'files').click();
  await expect(drawer).toHaveCount(0);

  // The left column opens from the left, and a tab that the person picks closes it.
  await page.getByRole('button', { name: 'Workspaces' }).click();
  const left = page.getByRole('navigation', { name: 'Workspaces' });
  await expect(left).toBeVisible();
  await left.locator(`[data-side-tab="${made.tab}"]`).click();
  await expect(left).toHaveCount(0);

  // The palette takes the width of the screen.
  await page.locator('.cmdline').click();
  const palette = page.getByRole('dialog', { name: 'Command palette' });
  await expect(palette).toBeVisible();
  const wide = await palette.boundingBox();
  expect(wide?.width ?? 0).toBeGreaterThan(440 - 40);
  await page.keyboard.press('Escape');
  await expect(palette).toHaveCount(0);

  // More has the facts of the status bar, and Settings. The palette command opens them too.
  await dock(page, 'more').click();
  const more = page.getByRole('dialog', { name: 'More' });
  await expect(more).toContainText('Machine');
  await expect(more.getByRole('button', { name: 'Stop everything' })).toBeVisible();
  await more.getByRole('button', { name: 'Close' }).click();
  await expect(more).toHaveCount(0);
  await noSideScroll(page);
  expect(await violations(page, '.shell'), 'the desk').toEqual([]);
});

test('the Pair view has no side scroll and no serious accessibility violation @narrow', async ({
  page,
  request,
}) => {
  const { origin } = await core(request);
  await page.goto(`${origin}/`);
  await expect(page.getByRole('heading', { name: 'Pair this phone with the Mac' })).toBeVisible();
  await noSideScroll(page);
  expect(await violations(page, '.phone-page')).toEqual([]);
});

test('a closed socket shows the banner, and the app comes back with the core', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const banner = page.getByRole('alert').filter({ hasText: 'The Mac does not answer' });
  await expect(banner).toHaveCount(0);

  expect((await request.post('/halt')).ok()).toBe(true);
  await expect(banner).toBeVisible({ timeout: 20_000 });
  // The core is gone: nothing listens on its port.
  const down = await page.request.get(`${c.origin}/api/me`).then(
    () => false,
    () => true,
  );
  expect(down).toBe(true);

  expect((await request.post('/resume')).ok()).toBe(true);
  // The transport connects again by itself, and the cookie still pairs the phone.
  await expect(banner).toHaveCount(0, { timeout: 30_000 });
  await settings(page, 'phone');
  await expect(
    page.getByRole('dialog', { name: 'Settings' }).getByRole('textbox', { name: 'Name' }),
  ).toHaveValue('Test iPhone', { timeout: 20_000 });
});

test('Unpair this phone shows the view Pair', async ({ page, request }) => {
  const c = await core(request);
  await pair(page, c);
  await settings(page, 'phone');
  await page.getByRole('button', { name: 'Unpair this phone' }).click();
  await expect(page.getByRole('heading', { name: 'Pair this phone with the Mac' })).toBeVisible();
  const me = await page.request.get(`${c.origin}/api/me`);
  expect(me.status()).toBe(401);
});

// --- keys and touch on a pane -------------------------------------------------------

/**
 * Sends a touch event to an element. The WebKit of the tests has no `Touch` constructor, so the
 * event is a plain event with the list of the fingers, which is all that the surface reads.
 */
async function touch(
  page: Page,
  selector: string,
  type: 'touchstart' | 'touchmove' | 'touchend',
  points: readonly { x: number; y: number }[],
): Promise<void> {
  await page.locator(selector).evaluate(
    (element, args) => {
      const box = element.getBoundingClientRect();
      const fingers = args.points.map((point, at) => ({
        identifier: at + 1,
        clientX: box.left + point.x,
        clientY: box.top + point.y,
      }));
      const list = { length: fingers.length, item: (at: number) => fingers[at] ?? null };
      const event = new Event(args.type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'touches', { value: list });
      Object.defineProperty(event, 'changedTouches', { value: list });
      element.dispatchEvent(event);
    },
    { type, points },
  );
}

/** A tap on an element at a point of its box: down and up. */
async function tapAt(page: Page, selector: string, x: number, y: number): Promise<void> {
  await touch(page, selector, 'touchstart', [{ x, y }]);
  await touch(page, selector, 'touchend', []);
}

/**
 * A program that writes the bytes of its input to a log, one byte in hex for each line, with the
 * terminal in raw mode, so that Esc and Ctrl+C arrive as bytes.
 */
function byteLogger(log: string): string[] {
  return [
    '/usr/bin/python3',
    '-u',
    '-c',
    `import sys, tty
tty.setraw(0)
print("ready", flush=True)
log = open(sys.argv[1], "a")
while True:
    b = sys.stdin.buffer.read(1)
    if not b:
        break
    log.write(b.hex() + "\\n")
    log.flush()`,
    log,
  ];
}

/** The bytes in the log of `byteLogger`, as text. */
async function bytesOf(log: string): Promise<string> {
  const hex = (await readFile(log, 'utf8')).split('\n').filter(Boolean);
  return Buffer.from(hex.join(''), 'hex').toString('latin1');
}

test('the soft keyboard and the key bar type into the pane of a phone', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-keys-${process.pid}`);
  const log = join(dir, 'bytes.log');
  await mkdir(dir, { recursive: true });
  await writeFile(log, '');
  try {
    const made = await c.api<{ pane: string }>('workspace.create', {
      name: 'Keys',
      command: { argv: byteLogger(log) },
    });
    await page.goto(`${c.origin}/#/pane/${made.pane}`);
    await desk(page);
    const surface = `[data-pane="${made.pane}"] .pane-surface`;
    await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
    await expect(page.locator('.layout .pane.focused')).toHaveCount(1, { timeout: 20_000 });

    // A tap on the pane in focus opens the keyboard: the key bar takes the place of the dock.
    await tapAt(page, surface, 40, 40);
    await expect(page.locator('.key-bar')).toBeVisible();
    await expect(page.locator('.phone-dock')).toHaveCount(0);
    const field = page.locator(`[data-pane="${made.pane}"] .pane-input`);
    await expect(field).toBeFocused();
    await expect(field).toHaveCSS('font-size', '16px');

    // The soft keyboard: text, and a correction that replaces the word.
    await page.keyboard.insertText('teh');
    await field.evaluate((element) => {
      const input = element as HTMLTextAreaElement;
      input.value = 'the';
      input.dispatchEvent(new InputEvent('input', { inputType: 'insertReplacementText' }));
    });
    await page.keyboard.insertText(' x');
    await expect.poll(() => bytesOf(log), { timeout: 20_000 }).toBe('teh\x7f\x7fhe x');

    // The key bar: Esc, and Ctrl for the next key.
    const bar = page.locator('.key-bar');
    await bar.getByRole('button', { name: 'Escape' }).click();
    await bar.getByRole('button', { name: 'Ctrl' }).click();
    await expect(bar.getByRole('button', { name: 'Ctrl' })).toHaveAttribute('aria-pressed', 'true');
    await page.keyboard.insertText('c');
    await expect(bar.getByRole('button', { name: 'Ctrl' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    await bar.getByRole('button', { name: 'Up' }).click();
    await expect
      .poll(() => bytesOf(log), { timeout: 20_000 })
      .toBe('teh\x7f\x7fhe x\x1b\x03\x1b[A');
    // Hide closes the keyboard, and the dock comes back.
    await bar.getByRole('button', { name: 'Hide the keyboard' }).click();
    await expect(page.locator('.phone-dock')).toBeVisible();
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('a finger scrolls the history, and a long press selects a word', async ({ page, request }) => {
  const c = await core(request);
  await pair(page, c);
  const made = await c.api<{ pane: string }>('workspace.create', {
    name: 'Touch',
    command: { argv: ['/bin/sh', '-c', 'seq 1 300; echo LASTWORD here; exec sleep 600'] },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  const surface = `[data-pane="${made.pane}"] .pane-surface`;
  await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
  // One finger down the screen scrolls toward the start of the history: the core gets wheel
  // notches, and the pane shows an older line.
  await touch(page, surface, 'touchstart', [{ x: 100, y: 100 }]);
  for (let step = 1; step <= 10; step++) {
    await touch(page, surface, 'touchmove', [{ x: 100, y: 100 + step * 40 }]);
  }
  // The finger rests before it leaves, so the scroll does not glide on.
  await page.waitForTimeout(250);
  await touch(page, surface, 'touchend', []);
  await expect
    .poll(
      async () =>
        (await c.api<{ text: string }>('pane.read', { pane: made.pane, visible: true })).text,
      { timeout: 20_000 },
    )
    .not.toContain('LASTWORD');

  // Back at the end, a long press on the last word selects it, and Copy is on the bar.
  await c.api('pane.scroll', { pane: made.pane, delta: 100_000 });
  await expect
    .poll(
      async () =>
        (await c.api<{ text: string }>('pane.read', { pane: made.pane, visible: true })).text,
      { timeout: 20_000 },
    )
    .toContain('LASTWORD');
  const lines = (
    await c.api<{ text: string }>('pane.read', { pane: made.pane, visible: true })
  ).text.split('\n');
  const row = lines.findIndex((line) => line.includes('LASTWORD'));
  // The rows of the surface start at its top: the row of the word is at `row` cells. The field
  // of the pane has the height of a cell.
  const cellHeight = await page.locator(surface).evaluate((element) => {
    const input = element.querySelector('.pane-input') as HTMLElement;
    return Number.parseFloat(input.style.height);
  });
  await touch(page, surface, 'touchstart', [{ x: 12, y: (row + 0.5) * cellHeight }]);
  await page.waitForTimeout(700);
  await touch(page, surface, 'touchend', []);
  await expect(page.getByRole('toolbar', { name: 'Selection' })).toBeVisible();
  await expect
    .poll(
      async () =>
        (await c.api<{ text?: string }>('pane.copy', { pane: made.pane, return_text: true })).text,
      { timeout: 20_000 },
    )
    .toBe('LASTWORD');
});

/** A call with the scope of the UI over the dev WebSocket: what a window of the desk sends. */
async function deskCall<T = Record<string, unknown>>(
  info: Info,
  method: string,
  params: Record<string, unknown>,
): Promise<T> {
  const socket = new WebSocket(`ws://127.0.0.1:${info.ws_port}/rpc?token=${info.token}`);
  await new Promise<void>((resolve, reject) => {
    socket.onopen = () => resolve();
    socket.onerror = () => reject(new Error('the dev WebSocket did not open'));
  });
  try {
    return await new Promise<T>((resolve, reject) => {
      socket.onmessage = (event) => {
        const reply = JSON.parse(String(event.data)) as {
          id?: number;
          result?: T;
          error?: { message: string };
        };
        if (reply.id !== 1) return;
        if (reply.error) reject(new Error(`${method}: ${reply.error.message}`));
        else resolve(reply.result as T);
      };
      socket.send(JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }));
    });
  } finally {
    socket.close();
  }
}

/** The size of a pane and the window that owns it, from `pane.show`. */
async function sizeOf(c: Core, pane: string) {
  const shown = await c.api<{
    pane: { cols: number; rows: number; geometry_owner?: string | null };
  }>('pane.show', { pane });
  return shown.pane;
}

test('the keyboard of the phone claims the size of the pane', async ({ page, request }) => {
  const c = await core(request);
  await pair(page, c);
  const made = await c.api<{ pane: string; tab: string }>('workspace.create', {
    name: 'Claim',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  const surface = `[data-pane="${made.pane}"] .pane-surface`;
  await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
  await expect(page.locator('.layout .pane.focused')).toHaveCount(1, { timeout: 20_000 });
  await expect.poll(async () => (await sizeOf(c, made.pane)).geometry_owner ?? null).toBeTruthy();
  const phoneSize = await sizeOf(c, made.pane);

  // A window of the desk takes the size: a larger grid than the box of the phone.
  const mac = (await deskCall<{ window: string }>(c.info, 'window.open', { tab: made.tab })).window;
  await deskCall(c.info, 'pane.resize', { pane: made.pane, cols: 160, rows: 60, window: mac });
  await page.waitForTimeout(600);
  await deskCall(c.info, 'pane.claim', { pane: made.pane, window: mac });
  await expect.poll(async () => (await sizeOf(c, made.pane)).cols).toBe(160);
  // The strip of the phone has the mark Fit.
  await expect(page.locator(`[data-pane="${made.pane}"] [data-action="fit"]`)).toBeVisible();

  // The keyboard opens: the phone claims the lease, and the PTY gets the size of the phone.
  await page.waitForTimeout(600);
  await tapAt(page, surface, 40, 40);
  await expect(page.locator('.key-bar')).toBeVisible();
  await expect
    .poll(async () => (await sizeOf(c, made.pane)).geometry_owner)
    .toBe(phoneSize.geometry_owner);
  await expect.poll(async () => (await sizeOf(c, made.pane)).cols).toBe(phoneSize.cols);
  await deskCall(c.info, 'window.close', { window: mac });
});

test('Compose sends the next prompt to an agent that waits for it', async ({ page, request }) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-compose-${process.pid}`);
  const log = join(dir, 'input.log');
  const stop = () => exec('pkill', ['-9', '-f', '^claude -u -c']).catch(() => undefined);
  try {
    await mkdir(dir, { recursive: true });
    await writeFile(log, '');
    const session = 's-phone-compose';
    const script = await program(
      dir,
      c.info.nh,
      [
        { event: 'SessionStart', payload: { session_id: session, cwd: dir, source: 'startup' } },
        { event: 'UserPromptSubmit', payload: { session_id: session, prompt: 'start' } },
        { event: 'Stop', payload: { session_id: session } },
      ],
      `/usr/bin/python3 -u -c 'import sys
log = open(sys.argv[1], "a")
for line in sys.stdin:
    log.write(line)
    log.flush()' '${log}'`,
    );
    const made = await c.api<{ pane: string }>('workspace.create', {
      name: 'Compose',
      command: { argv: [script] },
    });
    await page.goto(`${c.origin}/#/pane/${made.pane}`);
    await desk(page);
    const surface = `[data-pane="${made.pane}"] .pane-surface`;
    await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
    await expect(page.locator('.layout .pane.focused')).toHaveCount(1, { timeout: 20_000 });
    // The run waits for its next prompt: its turn ended.
    await expect
      .poll(
        async () =>
          (
            await c.api<{
              agents: {
                pane?: string | null;
                facets: { execution: { value: { state: string } } };
              }[];
            }>('agent.list', {})
          ).agents.find((agent) => agent.pane === made.pane)?.facets.execution.value.state,
        { timeout: 30_000 },
      )
      .toBe('idle');

    await tapAt(page, surface, 40, 40);
    const bar = page.locator('.key-bar');
    await bar.getByRole('button', { name: 'Compose' }).click();
    const compose = bar.getByRole('form', { name: 'Compose' });
    await compose.getByRole('textbox').fill('a longer prompt from the phone');
    await compose.getByRole('button', { name: 'Send' }).click();
    await expect
      .poll(async () => await readFile(log, 'utf8'), { timeout: 20_000 })
      .toContain('a longer prompt from the phone');
    expect((await readFile(log, 'utf8')).match(/a longer prompt from the phone/g)).toHaveLength(1);
    await expect(compose).toHaveCount(0);
  } finally {
    await stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('a key that the phone sends while the core is down arrives once after the reconnect', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-outbox-${process.pid}`);
  const log = join(dir, 'bytes.log');
  await mkdir(dir, { recursive: true });
  await writeFile(log, '');
  try {
    const made = await c.api<{ pane: string }>('workspace.create', {
      name: 'Outbox',
      command: { argv: byteLogger(log) },
    });
    await page.goto(`${c.origin}/#/pane/${made.pane}`);
    await desk(page);
    const surface = `[data-pane="${made.pane}"] .pane-surface`;
    await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
    await expect(page.locator('.layout .pane.focused')).toHaveCount(1, { timeout: 20_000 });
    await tapAt(page, surface, 40, 40);
    await expect(page.locator('.key-bar')).toBeVisible();

    // The core dies, and its holders keep the panes. A key goes into the outbox, and the transport
    // sends it when the socket is back and the pane is restored. The holder applies its id once.
    expect((await request.post('/crash')).ok()).toBe(true);
    await expect(
      page.getByRole('alert').filter({ hasText: 'The Mac does not answer' }),
    ).toBeVisible({
      timeout: 20_000,
    });
    await page.locator(`[data-pane="${made.pane}"] .pane-input`).focus();
    await page.keyboard.insertText('Q');
    expect((await request.post('/resume')).ok()).toBe(true);
    await expect.poll(() => bytesOf(log), { timeout: 30_000 }).toBe('Q');
    // Nothing more comes: no second copy of the key.
    await page.waitForTimeout(1500);
    expect(await bytesOf(log)).toBe('Q');
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

// --- touch for the chrome and the native integrations -------------------------------

/** A press of a finger on an element, as pointer events: down, and up after `ms`. */
async function press(page: Page, selector: string, ms: number): Promise<{ x: number; y: number }> {
  const box = await page.locator(selector).first().boundingBox();
  if (!box) throw new Error(`${selector} has no box`);
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const init = {
    pointerType: 'touch',
    pointerId: 7,
    isPrimary: true,
    button: 0,
    clientX: x,
    clientY: y,
    bubbles: true,
  };
  await page.locator(selector).first().dispatchEvent('pointerdown', init);
  await page.waitForTimeout(ms);
  return { x, y };
}

async function lift(page: Page, x: number, y: number): Promise<void> {
  await page.evaluate(
    ([px, py]) =>
      window.dispatchEvent(
        new PointerEvent('pointerup', {
          pointerType: 'touch',
          pointerId: 7,
          isPrimary: true,
          button: 0,
          clientX: px,
          clientY: py,
          bubbles: true,
        }),
      ),
    [x, y] as const,
  );
}

test('a long press opens the menus of the chrome, and a tab moves from its menu', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const made = await c.api<{ workspace: string; pane: string; tab: string }>('workspace.create', {
    name: 'Menus',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  const second = await c.api<{ tab: string }>('tab.create', {
    workspace: made.workspace,
    name: 'Second',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  await expect(page.locator('.tabbar .tab')).toHaveCount(2, { timeout: 20_000 });

  // A long press on a tab opens its menu: Rename, a move and Close.
  const at = await press(page, `.tabbar [data-tab="${made.tab}"]`, 650);
  await lift(page, at.x, at.y);
  const menu = page.getByRole('menu', { name: /Actions of/ });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole('menuitem')).toHaveText(['Rename', 'Move right', 'Close']);
  expect(await violations(page, '.shell'), 'the menu').toEqual([]);
  await menu.getByRole('menuitem', { name: 'Move right' }).click();
  await expect(page.locator('.tabbar .tab').first()).toHaveAttribute('data-tab', second.tab, {
    timeout: 20_000,
  });

  // A short press that moves is a scroll: no menu and no drag.
  await press(page, `.tabbar [data-tab="${made.tab}"]`, 50);
  await lift(page, at.x, at.y);
  await page.waitForTimeout(700);
  await expect(menu).toHaveCount(0);

  // A long press on a workspace of the left column opens Rename and Close.
  await page.getByRole('button', { name: 'Workspaces' }).click();
  const title = `.sidebar [data-workspace="${made.workspace}"] .workspace-title`;
  const here = await press(page, title, 650);
  await lift(page, here.x, here.y);
  await expect(menu.getByRole('menuitem')).toHaveText(['Rename', 'Close']);
  await menu.getByRole('menuitem', { name: 'Rename' }).click();
  const field = page
    .getByRole('navigation', { name: 'Workspaces' })
    .getByRole('textbox', { name: /Name of workspace/ });
  await expect(field).toBeFocused();
  await expect(field).toHaveCSS('font-size', '16px');
  await page.keyboard.press('Escape');
});

test('a file attaches to a pane, and a row of the Files tab opens only in the large view on the phone', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const dir = join(tmpdir(), `nh-phone-files-${process.pid}`);
  await mkdir(dir, { recursive: true });
  await writeFile(join(dir, 'notes.txt'), 'hello\n');
  try {
    const made = await c.api<{ pane: string }>('workspace.create', {
      name: 'Files',
      root: dir,
      command: { argv: ['/bin/sh', '-c', `cd '${dir}'; exec cat`] },
    });
    await page.goto(`${c.origin}/#/pane/${made.pane}`);
    await desk(page);
    await expect(page.locator(`[data-pane="${made.pane}"] .pane-surface`)).toBeVisible({
      timeout: 20_000,
    });

    // The menu ⋯ of the pane has Attach a file, and the picker of iOS gives the files.
    await page
      .locator(`[data-pane="${made.pane}"]`)
      .getByRole('button', { name: 'Pane actions' })
      .click();
    const menu = page.getByRole('menu', { name: /Actions of/ });
    await expect(menu.getByRole('menuitem', { name: 'Attach a file' })).toBeVisible();
    await expect(menu.getByRole('menuitem', { name: 'Fit to this phone' })).toBeVisible();
    // A tap beside the menu closes it.
    await page.mouse.click(5, 5);
    await expect(menu).toHaveCount(0);
    await page.locator(`[data-pane="${made.pane}"] .pane-attach`).setInputFiles({
      name: 'shot.png',
      mimeType: 'image/png',
      buffer: Buffer.from('\x89PNG\r\n\x1a\nfake'),
    });
    await expect
      .poll(
        async () =>
          (await c.api<{ text: string }>('pane.read', { pane: made.pane, visible: true })).text,
        { timeout: 20_000 },
      )
      .toContain('shot.png');

    // A long press on a row of Files: Open shows the file on the phone. Open in app and Reveal act on
    // the screen of the Mac, so the menu does not have them.
    await dock(page, 'files').click();
    // The Files tab shows the folder of the pane, which is the real path (`/private/var` on a Mac).
    const row = `.files-row[data-path="${join(await realpath(dir), 'notes.txt')}"]`;
    await expect(page.locator(row)).toBeVisible({ timeout: 20_000 });
    const at = await press(page, row, 650);
    await lift(page, at.x, at.y);
    const rowMenu = page.getByRole('menu', { name: 'Actions of notes.txt' });
    await expect(rowMenu).toBeVisible();
    await expect(rowMenu.getByRole('menuitem')).toHaveText([
      'Open',
      'Copy path',
      'Copy relative path',
      'Insert into pane',
    ]);
    // Open shows the file in a large view that takes the screen.
    await rowMenu.getByRole('menuitem', { name: 'Open' }).click();
    const large = page.getByRole('dialog', { name: 'notes.txt' });
    await expect(large.locator('.file-body')).toBeVisible();
    // The view grows in when it opens.
    await expect
      .poll(async () => (await large.boundingBox())?.width)
      .toBe(page.viewportSize()?.width);
    await expect(large.getByRole('button', { name: 'Open in app' })).toHaveCount(0);
    await large.getByRole('button', { name: 'Close the large view' }).click();
    await expect(large).toHaveCount(0);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('a remembered clipboard write of a program waits for a tap on Copy', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const write = (text: string): string =>
    `printf '\\033]52;c;%s\\007' "$(printf '${text}' | base64)"`;
  const made = await c.api<{ pane: string }>('workspace.create', {
    name: 'Clipboard',
    command: {
      argv: [
        '/bin/sh',
        '-c',
        `echo ready; read a; ${write('first text')}; read b; ${write('second text')}; exec sleep 600`,
      ],
    },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  const surface = `[data-pane="${made.pane}"] .pane-surface`;
  await expect(page.locator(surface)).toBeVisible({ timeout: 20_000 });
  await expect(page.locator('.layout .pane.focused')).toHaveCount(1, { timeout: 20_000 });

  // The first write asks: Allow, and remember it for the pane.
  await tapAt(page, surface, 40, 40);
  await page.keyboard.press('Enter');
  const strip = page.locator(`[data-pane="${made.pane}"] .pane-clipboard`);
  await expect(strip).toContainText('first text', { timeout: 20_000 });
  await strip.getByRole('checkbox').check();
  await strip.getByRole('button', { name: 'Allow' }).click();
  await expect(strip).toHaveCount(0);

  // A later write needs a tap all the same, because only a tap writes the clipboard of a phone:
  // the strip has Copy, and no box Remember.
  await tapAt(page, surface, 40, 40);
  await page.keyboard.press('Enter');
  await expect(strip).toContainText('second text', { timeout: 20_000 });
  await expect(strip.getByRole('checkbox')).toHaveCount(0);
  await strip.getByRole('button', { name: 'Copy' }).click();
  await expect(strip).toHaveCount(0);
});

test('the left column lists the machines, and a machine that does not answer has a way back', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  await page.goto(`${c.origin}/#/`);
  await desk(page);

  // The left column ends with the machines of the Mac. The desk shows this Mac.
  await page.getByRole('button', { name: 'Workspaces' }).click();
  const machines = page.getByRole('region', { name: 'Machines' });
  await expect(machines).toBeVisible();
  const mac = machines.locator('[data-machine="local"]');
  await expect(mac).toContainText('This Mac');
  await expect(mac).toHaveAttribute('aria-current', 'true');
  await expect(mac.getByRole('button', { name: 'Show' })).toHaveCount(0);
  expect(await violations(page, '.sidebar'), 'the left column').toEqual([]);

  // A push of a machine opens `#/machine/<label>`. This listener reaches no other machine, so
  // the desk waits for it and offers the way back.
  await page.goto(`${c.origin}/#/machine/studio`);
  await expect(page.getByRole('status')).toContainText('Connecting to studio', {
    timeout: 20_000,
  });
  expect(new URL(page.url()).hash).toBe('#/');
  await page.getByRole('button', { name: 'Back to this Mac' }).click();
  await desk(page);
  await page.getByRole('button', { name: 'Workspaces' }).click();
  await expect(
    page.getByRole('region', { name: 'Machines' }).locator('[data-machine="local"]'),
  ).toHaveAttribute('aria-current', 'true');
});

test('Escape closes each menu and sheet of the desk, and the focus goes back to its button', async ({
  page,
  request,
}) => {
  const c = await core(request);
  await pair(page, c);
  const made = await c.api<{ pane: string; tab: string }>('workspace.create', {
    name: 'Layers',
    command: { argv: ['/bin/sh', '-c', 'exec sleep 600'] },
  });
  await page.goto(`${c.origin}/#/pane/${made.pane}`);
  await desk(page);
  const tab = page.locator(`.tabbar [data-tab="${made.tab}"]`);
  await expect(tab).toBeVisible({ timeout: 20_000 });

  // The menu ⋯ of a pane gives the focus back to its button.
  const actions = page
    .locator(`[data-pane="${made.pane}"]`)
    .getByRole('button', { name: 'Pane actions' });
  await actions.click();
  const menu = page.getByRole('menu', { name: /Actions of/ });
  await expect(menu).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(actions).toBeFocused();

  // The menu of a long press gives the focus back to the tab that had it.
  await tab.focus();
  const at = await press(page, `.tabbar [data-tab="${made.tab}"]`, 650);
  await lift(page, at.x, at.y);
  await expect(menu).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(tab).toBeFocused();

  // More, the drawer and the left column: each sheet that a button opened gives it the focus back.
  const sheets = [
    { button: dock(page, 'more'), sheet: page.getByRole('dialog', { name: 'More' }) },
    { button: dock(page, 'inbox'), sheet: page.getByRole('dialog', { name: 'Drawer' }) },
    {
      button: page.getByRole('button', { name: 'Workspaces' }),
      sheet: page.getByRole('dialog', { name: 'Workspaces' }),
    },
  ];
  for (const { button, sheet } of sheets) {
    await button.focus();
    await page.keyboard.press('Enter');
    await expect(sheet).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(sheet).toHaveCount(0);
    await expect(button).toBeFocused();
  }
});
