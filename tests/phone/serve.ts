// Starts a headless core with the listener of the phone for the Playwright spec of the phone app.
// Run it with `bun tests/phone/serve.ts`. The Playwright config starts it.
//
// It builds the phone app into `target/web-dist`, builds `nh` with it (`NEOHAR_WEB_DIST`), and
// starts `nh daemon --dev-ws --phone-port 0` (`tests/support/daemon.ts`). The listener serves the
// page itself at `http://127.0.0.1:<phone_port>/`. This server says where (`/info`). It also stops
// the core (`POST /halt`), kills it while its holders keep running (`POST /crash`), and starts it
// again on the same directories and port (`POST /resume`).

import { join } from 'node:path';
import { root, run, startDaemon } from '../support/daemon';

const port = Number(process.env.NEOHAR_UI_PORT ?? 4178);

await run(['bun', 'run', '--cwd', 'apps/web', 'build']);
const daemon = await startDaemon('nh-phone', {
  phonePort: 0,
  webDist: join(root, 'target', 'web-dist'),
});

const server = Bun.serve({
  port,
  hostname: '127.0.0.1',
  async fetch(request) {
    const url = new URL(request.url);
    const info = () => ({ ...daemon.info, nh: daemon.nh, config_dir: daemon.configDir });
    if (url.pathname === '/ready') return new Response('ok');
    if (url.pathname === '/info') return Response.json(info());
    if (url.pathname === '/halt' && request.method === 'POST') {
      await daemon.halt();
      return Response.json({});
    }
    if (url.pathname === '/crash' && request.method === 'POST') {
      await daemon.crash();
      return Response.json({});
    }
    if (url.pathname === '/resume' && request.method === 'POST') {
      await daemon.resume();
      return Response.json(info());
    }
    return new Response('not found', { status: 404 });
  },
});
console.log(`phone test server on ${server.url}`);

let ending = false;
async function end(): Promise<void> {
  if (ending) return;
  ending = true;
  server.stop(true);
  await daemon.stop();
  process.exit(0);
}
for (const signal of ['SIGINT', 'SIGTERM'] as const) process.on(signal, () => void end());
