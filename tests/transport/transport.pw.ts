// The WebSocketTransport in a browser against a headless core: the page subscribes to
// `tree`, gets a snapshot, and then gets a patch when the CLI makes a workspace.

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { expect, test } from '@playwright/test';

const exec = promisify(execFile);

interface Info {
  ws_port: number;
  token: string;
  api_socket: string;
  nh: string;
}

interface Seen {
  kind: string;
  version: number;
  keys: string[];
}

test('a snapshot, then a patch after a workspace of the CLI, and calls with typed results', async ({
  page,
  request,
}) => {
  const info = (await (await request.get('/info')).json()) as Info;
  await page.goto(`/page.html?ws=${info.ws_port}&token=${info.token}`);

  // The core has no workspace: the snapshot is empty.
  await expect
    .poll(async () => page.evaluate(() => (window as never as NhTest).nhTest.seen().length))
    .toBeGreaterThan(0);
  const first = await page.evaluate(() => (window as never as NhTest).nhTest.seen());
  expect(first[0]).toMatchObject({ kind: 'snapshot', keys: [] });

  // A workspace made on the socket of the CLI reaches the page as a patch.
  const made = await exec(info.nh, [
    '--socket',
    info.api_socket,
    'api',
    'call',
    'workspace.create',
    JSON.stringify({ name: 'From the CLI' }),
  ]);
  const created = JSON.parse(made.stdout) as { workspace: string };
  await expect
    .poll(async () =>
      page.evaluate(() =>
        (window as never as NhTest).nhTest.seen().some((s) => s.kind === 'patch'),
      ),
    )
    .toBe(true);
  const seen = await page.evaluate(() => (window as never as NhTest).nhTest.seen());
  const patch = seen.find((s) => s.kind === 'patch');
  expect(patch?.keys).toContain(created.workspace);
  expect(patch?.version).toBe((first[0]?.version ?? 0) + 1);

  // A call on the same socket gives the same workspace, and an error has its code.
  const list = await page.evaluate(() =>
    (window as never as NhTest).nhTest.call('workspace.list', {}),
  );
  expect(list).toMatchObject({ workspaces: [{ name: 'From the CLI', handle: 'w1' }] });
  const failure = await page.evaluate(() =>
    (window as never as NhTest).nhTest
      .call('workspace.show', { workspace: 'w9' })
      .then(() => null)
      .catch((error: { code: string; message: string }) => ({
        code: error.code,
        message: error.message,
      })),
  );
  expect(failure?.code).toBe('not_found');
});

interface NhTest {
  nhTest: {
    seen: () => Seen[];
    resyncs: () => number[];
    call: (method: string, params: Record<string, unknown>) => Promise<unknown>;
    close: () => void;
  };
}
