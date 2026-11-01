import { afterAll, describe, expect, it } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BUNDLES, firstFiles, LIMIT, measure, PHONE_LIMIT, table, total } from './check-bundle';

const dir = mkdtempSync(join(tmpdir(), 'nh-bundle-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('measure', () => {
  it('counts scripts, styles and WASM, and skips maps and the page', () => {
    mkdirSync(join(dir, 'assets'), { recursive: true });
    writeFileSync(join(dir, 'index.html'), '<html></html>');
    writeFileSync(join(dir, 'assets', 'a.js'), 'x'.repeat(10_000));
    writeFileSync(join(dir, 'assets', 'a.js.map'), 'm'.repeat(50_000));
    writeFileSync(join(dir, 'assets', 'b.css'), 'body{}'.repeat(100));
    writeFileSync(join(dir, 'assets', 'c.wasm'), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
    const parts = measure(dir);
    expect(parts.map((part) => part.file).sort()).toEqual([
      'assets/a.js',
      'assets/b.css',
      'assets/c.wasm',
    ]);
    const script = parts.find((part) => part.file === 'assets/a.js');
    expect(script?.raw).toBe(10_000);
    // Repeated text compresses well.
    expect(script?.gzip).toBeLessThan(200);
  });

  it('orders the parts by size and adds them up', () => {
    const parts = measure(dir);
    const sizes = parts.map((part) => part.gzip);
    expect(sizes).toEqual([...sizes].sort((a, b) => b - a));
    expect(total(parts)).toBe(sizes.reduce((sum, size) => sum + size, 0));
    expect(total(parts)).toBeLessThan(LIMIT);
  });
});

describe('table', () => {
  it('ends with the total and the limit', () => {
    const lines = table(measure(dir)).split('\n');
    expect(lines.at(-1)).toContain('limit');
    expect(lines.at(-1)).toContain(String(LIMIT));
    expect(lines).toHaveLength(4);
  });
});

describe('the limit', () => {
  it('is 400 KB of gzipped bytes', () => {
    expect(LIMIT).toBe(400_000);
  });

  it('is 400 KB for the phone app, and 250 KB for its first chunk', () => {
    expect(PHONE_LIMIT).toBe(250_000);
    expect(BUNDLES.map((bundle) => [bundle.name, bundle.limit, bundle.first])).toEqual([
      ['desk', LIMIT, null],
      ['phone', LIMIT, PHONE_LIMIT],
    ]);
    expect(table([], PHONE_LIMIT)).toContain(String(PHONE_LIMIT));
  });

  it('the first chunk is what the page loads before its first paint', () => {
    const page = [
      '<link rel="stylesheet" href="/theme.css" />',
      '<script type="module" crossorigin src="/assets/main-1.js"></script>',
      '<link rel="modulepreload" crossorigin href="/assets/react-2.js">',
      '<link rel="stylesheet" crossorigin href="/assets/main-3.css">',
    ].join('\n');
    expect(firstFiles(page)).toEqual([
      'assets/main-1.js',
      'assets/main-3.css',
      'assets/react-2.js',
    ]);
  });
});
