// Builds the test pages and starts `nh-keytest` for the Playwright spec.
//
// Run it with `bun tests/keyboard/serve.ts`. The Playwright config starts it. The pages are built
// into `target/keytest-web`. It needs `cargo` on the PATH (`mise run test:keyboard` provides it).

import { cp, mkdir } from 'node:fs/promises';
import { join } from 'node:path';

const root = join(import.meta.dir, '..', '..');
const dist = join(root, 'target', 'keytest-web');
const port = process.env.NEOHAR_KEYTEST_PORT ?? '4174';

async function run(cmd: string[]): Promise<void> {
  const p = Bun.spawn(cmd, { cwd: root, stdout: 'inherit', stderr: 'inherit' });
  if ((await p.exited) !== 0) throw new Error(`${cmd.join(' ')} failed`);
}

await mkdir(dist, { recursive: true });
const built = await Bun.build({
  entrypoints: [
    join(root, 'tools/nh-keytest/web/test.ts'),
    join(root, 'tools/nh-keytest/web/capture.ts'),
  ],
  outdir: dist,
  target: 'browser',
  sourcemap: 'inline',
});
if (!built.success) {
  for (const message of built.logs) console.error(message);
  throw new Error('the test pages do not build');
}
for (const page of ['test.html', 'capture.html']) {
  await cp(join(root, 'tools/nh-keytest/web', page), join(dist, page));
}
await run(['cargo', 'build', '--locked', '-p', 'nh-keytest']);

const server = Bun.spawn(
  [
    join(root, 'target/debug/nh-keytest'),
    '--web',
    dist,
    '--cases',
    join(root, 'tests/keyboard/cases'),
    '--port',
    port,
  ],
  { cwd: root, stdout: 'inherit', stderr: 'inherit' },
);
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => {
    server.kill();
    process.exit(0);
  });
}
process.exit(await server.exited);
