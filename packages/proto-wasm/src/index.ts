import init, { WasmDecoder } from '../pkg/proto_wasm.js';

/**
 * `decode` applied the frame. A result that is not this one or `NEEDS_KEYFRAME` means that the
 * bytes are not a frame.
 */
export const APPLIED = 0;
/** The pane needs a keyframe: list it in `render.ack.need_keyframe`. */
export const NEEDS_KEYFRAME = 1;

type Source = Parameters<typeof init>[0];
/** What `createDecoder` returns. */
export type Decoder = Awaited<ReturnType<typeof createDecoder>>;
type Ctor<T> = new (buffer: ArrayBuffer, offset: number, length: number) => T;

/**
 * Loads the decoder. The arrays it returns are views of WASM memory. A view is made again when
 * the memory grows (the old views are detached) or the array moves. Keep no view across a call of
 * `decode`.
 */
export async function createDecoder(wasm: Source, atlasSlots: number) {
  const { memory } = await init({ module_or_path: wasm });
  const d = new WasmDecoder(atlasSlots);
  const views = new Map<number, { buffer: ArrayBuffer; ptr: number; len: number; view: unknown }>();
  const view = <T>(key: number, C: Ctor<T>, ptr: number, len: number): T => {
    let v = views.get(key);
    if (!v || v.buffer !== memory.buffer || v.ptr !== ptr || v.len !== len) {
      v = { buffer: memory.buffer, ptr, len, view: new C(memory.buffer, ptr, len) };
      views.set(key, v);
    }
    return v.view as T;
  };
  return {
    /** The getters of the decoder: sizes, cursor, modes, viewport, selection and so on. */
    raw: d,
    decode(frame: Uint8Array): number {
      const ptr = d.input_ptr(frame.length);
      view(-1, Uint8Array, ptr, d.input_cap()).set(frame);
      return d.decode_input(frame.length);
    },
    dirtyRows: () => view(-2, Uint16Array, d.dirty_rows_ptr(), d.dirty_rows_len()),
    cells: (p: number) => view(p * 8, Uint32Array, d.cells_ptr(p), d.cols(p) * d.rows(p)),
    styles: (p: number) => view(p * 8 + 1, Uint16Array, d.styles_ptr(p), d.cols(p) * d.rows(p)),
    styleTable: (p: number) =>
      view(p * 8 + 2, Uint32Array, d.style_table_ptr(p), d.style_table_len(p) * 4),
    pending: () => view(-3, Uint32Array, d.pending_ptr(), d.pending_len() * 4),
    title: (p: number) => view(p * 8 + 3, Uint8Array, d.title_ptr(p), d.title_len(p)),
    /** The text of a pending glyph that has the high bit set (a cluster). It names no pane. */
    atlasText: (glyph: number) =>
      view(-4, Uint8Array, d.atlas_text_ptr(glyph), d.atlas_text_len(glyph)),
    grapheme: (p: number, id: number) =>
      view(p * 8 + 4, Uint8Array, d.grapheme_ptr(p, id), d.grapheme_len(p, id)),
  };
}

export { frame_proto as frameProto } from '../pkg/proto_wasm.js';
