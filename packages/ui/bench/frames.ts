// The file that `nh-bench gen-frames` writes: a header, then records of tick, pane and frame.

/** The magic number at the start of the file. */
const MAGIC = 'NHFRAMES';

export interface FrameFile {
  panes: number;
  cols: number;
  rows: number;
  /** The number of display frames. */
  ticks: number;
  /** The first record of each tick, and one more at the end. */
  tickStart: Uint32Array;
  /** The pane of each record. */
  paneOf: Uint32Array;
  /** The bytes of each record. They are views of the file, so no frame is copied. */
  frames: Uint8Array[];
}

/** Reads a file. It throws when the bytes are not a frame file. */
export function parseFrames(buffer: ArrayBuffer): FrameFile {
  const view = new DataView(buffer);
  if (buffer.byteLength < 28) throw new Error('the frame file is too short');
  for (let i = 0; i < MAGIC.length; i++) {
    if (view.getUint8(i) !== MAGIC.charCodeAt(i)) throw new Error('not a frame file');
  }
  if (view.getUint32(8, true) !== 1) throw new Error('the version of the frame file is unknown');
  const panes = view.getUint32(12, true);
  const cols = view.getUint32(16, true);
  const rows = view.getUint32(20, true);
  const count = view.getUint32(24, true);
  const paneOf = new Uint32Array(count);
  const ticksOf = new Uint32Array(count);
  const frames: Uint8Array[] = [];
  let at = 28;
  for (let i = 0; i < count; i++) {
    if (at + 12 > buffer.byteLength) throw new Error('the frame file ends early');
    ticksOf[i] = view.getUint32(at, true);
    paneOf[i] = view.getUint32(at + 4, true);
    const length = view.getUint32(at + 8, true);
    at += 12;
    if (at + length > buffer.byteLength) throw new Error('the frame file ends early');
    frames.push(new Uint8Array(buffer, at, length));
    at += length;
  }
  const ticks = count === 0 ? 0 : ticksOf[count - 1]! + 1;
  const tickStart = new Uint32Array(ticks + 1);
  let record = 0;
  for (let tick = 0; tick < ticks; tick++) {
    tickStart[tick] = record;
    while (record < count && ticksOf[record] === tick) record++;
  }
  tickStart[ticks] = count;
  return { panes, cols, rows, ticks, tickStart, paneOf, frames };
}
