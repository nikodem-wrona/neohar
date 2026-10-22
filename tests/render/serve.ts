// Builds the bench page and the frame files, and serves them for the Playwright specs.
//
// Run it with `bun tests/render/serve.ts`. The Playwright config starts it. The output is in
// `target/render-bench`. It needs `cargo` on the PATH (`mise run test:gpu` provides it).

import { cp, mkdir } from 'node:fs/promises';
import { join } from 'node:path';

const root = join(import.meta.dir, '..', '..');
const dist = join(root, 'target', 'render-bench');
const port = Number(process.env.NEOHAR_RENDER_PORT ?? 4173);

/** The frame files the specs use: name, then the arguments of `nh-bench gen-frames`. */
export const FRAMES: Record<string, string[]> = {
  // The fixture screen and ten variants of cursor and selection.
  screen: ['--scenario', 'screen', '--cols', '64', '--rows', '20'],
  // A small atlas fills up and resets.
  glyphs: ['--scenario', 'glyphs', '--cols', '40', '--rows', '10', '--frames', '12'],
  // 200x60 redraws of the whole pane.
  'redraw-200x60': ['--scenario', 'redraw', '--cols', '200', '--rows', '60', '--frames', '340'],
  // 20 panes with a spinner each, in a window of about 3400x2200 pixels: 5 across, 4 down.
  'spinner-20': [
    '--scenario',
    'spinner',
    '--panes',
    '20',
    '--cols',
    '40',
    '--rows',
    '16',
    '--frames',
    '740',
  ],
  // 20 panes that redraw everything, in the same window.
  'redraw-20': [
    '--scenario',
    'redraw',
    '--panes',
    '20',
    '--cols',
    '40',
    '--rows',
    '16',
    '--frames',
    '640',
  ],
  // A line of output in every display frame.
  'scroll-200x60': ['--scenario', 'scroll', '--cols', '200', '--rows', '60', '--frames', '640'],
};

async function run(cmd: string[]): Promise<void> {
  const p = Bun.spawn(cmd, { cwd: root, stdout: 'inherit', stderr: 'inherit' });
  if ((await p.exited) !== 0) throw new Error(`${cmd.join(' ')} failed`);
}

async function build(): Promise<void> {
  await mkdir(join(dist, 'frames'), { recursive: true });
  const built = await Bun.build({
    entrypoints: [join(root, 'packages/ui/bench/render.ts')],
    outdir: dist,
    naming: 'render.js',
    target: 'browser',
    sourcemap: 'inline',
  });
  if (!built.success) {
    for (const message of built.logs) console.error(message);
    throw new Error('the bench page does not build');
  }
  await cp(join(root, 'packages/ui/bench/render.html'), join(dist, 'render.html'));
  await cp(
    join(root, 'packages/proto-wasm/pkg/proto_wasm_bg.wasm'),
    join(dist, 'proto_wasm_bg.wasm'),
  );
  await run(['cargo', 'build', '--release', '--locked', '-p', 'nh-bench']);
  for (const [name, args] of Object.entries(FRAMES)) {
    await run([
      join(root, 'target/release/nh-bench'),
      'gen-frames',
      ...args,
      '--out',
      join(dist, 'frames', `${name}.frames`),
    ]);
  }
}

const types: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
};

await build();

Bun.serve({
  port,
  hostname: '127.0.0.1',
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/ready') return new Response('ready');
    const path = url.pathname === '/' ? '/render.html' : url.pathname;
    const file = Bun.file(join(dist, path));
    if (path.includes('..') || !(await file.exists()))
      return new Response('not found', { status: 404 });
    const extension = path.slice(path.lastIndexOf('.'));
    return new Response(file, {
      headers: {
        'content-type': types[extension] ?? 'application/octet-stream',
        // Cross-origin isolation gives timers of a finer resolution.
        'cross-origin-opener-policy': 'same-origin',
        'cross-origin-embedder-policy': 'require-corp',
        'cache-control': 'no-store',
      },
    });
  },
});
console.error(`render bench on http://127.0.0.1:${port}`);
