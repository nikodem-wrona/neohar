import { expect, test } from 'bun:test';
import { APPLIED, createDecoder, NEEDS_KEYFRAME } from './index';

/** What `decode` gives for bytes that are not a frame (`STATUS_ERROR` of the decoder). */
const ERROR = 2;

const here = (path: string) => new URL(path, import.meta.url);
const wasm = await Bun.file(here('../pkg/proto_wasm_bg.wasm')).arrayBuffer();
// Frozen fixtures of the frame format (`crates/nh-proto/tests/fixtures/frame`).
const fixture = async (name: string) =>
  new Uint8Array(
    await Bun.file(here(`../../../crates/nh-proto/tests/fixtures/frame/${name}.bin`)).arrayBuffer(),
  );

test('decodes the keyframe fixture into the arrays of its pane', async () => {
  const d = await createDecoder(wasm, 256);
  expect(d.decode(await fixture('frame_keyframe_full'))).toBe(APPLIED);
  expect(d.raw.pane()).toBe(7);
  expect(d.raw.rev()).toBe(31);
  expect([d.raw.cols(7), d.raw.rows(7)]).toEqual([5, 2]);
  expect(Array.from(d.dirtyRows())).toEqual([0, 1]);
  const cells = d.cells(7);
  expect(cells.length).toBe(10);
  // Slot 0 is empty, and the fixture draws letters, so every cell has a slot.
  expect(Array.from(cells).every((c) => (c & 0xffffff) !== 0)).toBe(true);
  expect(Array.from(d.styles(7))).toEqual(new Array(10).fill(1));
  // The pending glyphs say what to rasterize: h, e, l, o, w, r, d.
  const pending = d.pending();
  const glyphs = Array.from({ length: pending.length / 4 }, (_, i) =>
    String.fromCodePoint(pending[i * 4 + 1] ?? 0),
  );
  expect(glyphs.sort().join('')).toBe('dehlorw');
  expect(new TextDecoder().decode(d.title(7))).toBe('zsh');
  expect([
    d.raw.cursor_x(7),
    d.raw.cursor_y(7),
    d.raw.cursor_visible(7),
    d.raw.cursor_blink(7),
  ]).toEqual([2, 1, true, true]);
  expect(d.raw.bell(7)).toBe(true);
  expect(d.raw.viewport_top(7)).toBe(120);
  expect(d.raw.viewport_total(7)).toBe(4000);
  expect(d.raw.selection_mode(7)).toBe(4);
  expect(d.raw.wants_key_release(7)).toBe(false);
  // The style table has the default style and the style of the fixture, bold and curly underline.
  expect(d.styleTable(7).length).toBe(8);
  expect(d.raw.style_table_dirty(7)).toBe(true);
  d.raw.style_table_clean(7);
  expect(d.raw.style_table_dirty(7)).toBe(false);
});

test('a delta follows the keyframe, and without it the pane needs a keyframe', async () => {
  const d = await createDecoder(wasm, 256);
  const delta = await fixture('frame_delta_minimal');
  expect(d.decode(delta)).toBe(NEEDS_KEYFRAME);
  // The UI parses no bytes: the decoder names the pane for `render.ack.need_keyframe`.
  expect([d.raw.pane(), d.raw.rev()]).toEqual([7, 32]);
  expect(d.decode(await fixture('frame_keyframe_full'))).toBe(APPLIED);
  expect(d.decode(delta)).toBe(APPLIED);
  expect(d.raw.rev()).toBe(32);
  // Only row 1 changed, and the cursor sits on it.
  expect(Array.from(d.dirtyRows())).toEqual([1]);
  expect(d.raw.bell(7)).toBe(false);
});

test('bytes that are not a frame give an error', async () => {
  const d = await createDecoder(wasm, 256);
  expect(d.decode(new Uint8Array([1, 2, 3]))).toBe(ERROR);
  expect(d.decode(new Uint8Array(0))).toBe(ERROR);
});

test('a view stays correct when the memory grows', async () => {
  const d = await createDecoder(wasm, 256);
  d.decode(await fixture('frame_keyframe_full'));
  const before = Array.from(d.cells(7));
  const old = d.cells(7);
  // A big input makes WASM memory grow, which detaches every view over the old buffer.
  expect(d.decode(new Uint8Array(40 * 1024 * 1024))).toBe(ERROR);
  expect(old.length).toBe(0);
  expect(Array.from(d.cells(7))).toEqual(before);
});

test('the renderer reads dirty rows, pending glyphs and the palette without views', async () => {
  const d = await createDecoder(wasm, 256);
  d.decode(await fixture('frame_keyframe_full'));
  const rows = Array.from({ length: d.raw.dirty_rows_len() }, (_, i) => d.raw.dirty_row(i));
  expect(rows).toEqual([0, 1]);
  // Field 1 of a pending glyph is its code point, field 2 its variant.
  const letters = Array.from({ length: d.raw.pending_len() }, (_, i) =>
    String.fromCodePoint(d.raw.pending_field(i, 1)),
  );
  expect(letters.sort().join('')).toBe('dehlorw');
  const slots = Array.from({ length: d.raw.pending_len() }, (_, i) => d.raw.pending_field(i, 0));
  expect(new Set(slots).size).toBe(slots.length);
  expect(d.raw.palette_bg(7)).toBeGreaterThanOrEqual(0);
  expect(d.raw.palette_fg(7)).not.toBe(d.raw.palette_bg(7));
});

test('a view is the same object across frames while the memory and the size stay', async () => {
  const d = await createDecoder(wasm, 256);
  const keyframe = await fixture('frame_keyframe_full');
  d.decode(keyframe);
  const cells = d.cells(7);
  const styles = d.styles(7);
  d.decode(await fixture('frame_delta_minimal'));
  d.decode(keyframe);
  expect(d.cells(7)).toBe(cells);
  expect(d.styles(7)).toBe(styles);
});

test('the shim is at most 2 KB minified', async () => {
  const built = await Bun.build({
    entrypoints: [new URL('./index.ts', import.meta.url).pathname],
    minify: true,
    external: ['../pkg/*'],
    target: 'browser',
  });
  expect(built.success).toBe(true);
  const text = await built.outputs[0]?.text();
  expect((text ?? '').length).toBeLessThanOrEqual(2048);
});
