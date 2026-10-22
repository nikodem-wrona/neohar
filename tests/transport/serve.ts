// Starts a headless core and the page of the test for the Playwright spec.
//
// Run it with `bun tests/transport/serve.ts`. The Playwright config starts it. It starts the core
// (`tests/support/daemon.ts`), builds the page into `target/transport-web`, and serves the page
// and `/info` (where the core listens).

import { cp, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { root, startDaemon } from '../support/daemon';

const dist = join(root, 'target', 'transport-web');
const port = Number(process.env.NEOHAR_TRANSPORT_PORT ?? 4175);

await mkdir(dist, { recursive: true });
const built = await Bun.build({
  entrypoints: [join(import.meta.dir, 'page.ts')],
  outdir: dist,
  target: 'browser',
  sourcemap: 'inline',
});
if (!built.success) {
  for (const message of built.logs) console.error(message);
  throw new Error('the test page does not build');
}
await cp(join(import.meta.dir, 'page.html'), join(dist, 'page.html'));

const daemon = await startDaemon('nh-transport');
const { info, nh } = daemon;

const server = Bun.serve({
  port,
  hostname: '127.0.0.1',
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/ready') return new Response('ok');
    if (url.pathname === '/info') return Response.json({ ...info, nh });
    const file = Bun.file(join(dist, url.pathname === '/' ? 'page.html' : url.pathname));
    return (await file.exists()) ? new Response(file) : new Response('not found', { status: 404 });
  },
});
console.log(`transport test server on ${server.url}`);

let ending = false;
async function end(): Promise<void> {
  if (ending) return;
  ending = true;
  server.stop(true);
  await daemon.stop();
  process.exit(0);
}
for (const signal of ['SIGINT', 'SIGTERM'] as const) process.on(signal, () => void end());
