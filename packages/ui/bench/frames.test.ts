import { expect, test } from 'bun:test';
import { parseFrames } from './frames';

/** Builds a file like the one `nh-bench` writes. */
function file(records: Array<[tick: number, pane: number, bytes: number[]]>) {
  const size = 28 + records.reduce((n, r) => n + 12 + r[2].length, 0);
  const buffer = new ArrayBuffer(size);
  const view = new DataView(buffer);
  new Uint8Array(buffer).set(new TextEncoder().encode('NHFRAMES'));
  [1, 2, 80, 24, records.length].forEach((v, i) => view.setUint32(8 + i * 4, v, true));
  let at = 28;
  for (const [tick, pane, bytes] of records) {
    view.setUint32(at, tick, true);
    view.setUint32(at + 4, pane, true);
    view.setUint32(at + 8, bytes.length, true);
    new Uint8Array(buffer).set(bytes, at + 12);
    at += 12 + bytes.length;
  }
  return buffer;
}

test('records are grouped by display frame', () => {
  const f = parseFrames(
    file([
      [0, 1, [1, 2]],
      [0, 2, [3]],
      [1, 1, [4, 5, 6]],
      [2, 2, []],
    ]),
  );
  expect([f.panes, f.cols, f.rows, f.ticks]).toEqual([2, 80, 24, 3]);
  expect(Array.from(f.tickStart)).toEqual([0, 2, 3, 4]);
  expect(Array.from(f.paneOf)).toEqual([1, 2, 1, 2]);
  expect(f.frames.map((x) => Array.from(x))).toEqual([[1, 2], [3], [4, 5, 6], []]);
});

test('a file that is wrong or cut is refused', () => {
  const good = file([[0, 1, [1, 2, 3]]]);
  expect(() => parseFrames(new ArrayBuffer(4))).toThrow('too short');
  expect(() => parseFrames(good.slice(0, good.byteLength - 1))).toThrow('ends early');
  const wrong = good.slice(0);
  new Uint8Array(wrong)[0] = 0;
  expect(() => parseFrames(wrong)).toThrow('not a frame file');
});
