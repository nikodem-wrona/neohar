// A small PNG reader for the screenshots of Playwright (8 bits, RGB or RGBA, no interlace). The
// specs use it to count the pixels that a canvas painted. A screenshot has the pixels of the
// canvas, which the DOM cannot give.

import { inflateSync } from 'node:zlib';

export interface Image {
  width: number;
  height: number;
  /** Red, green, blue and alpha for each pixel, row by row. */
  data: Uint8Array;
}

export function decodePng(buffer: Buffer): Image {
  if (buffer.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');
  let width = 0;
  let height = 0;
  let channels = 0;
  const parts: Buffer[] = [];
  for (let at = 8; at < buffer.length;) {
    const length = buffer.readUInt32BE(at);
    const type = buffer.toString('latin1', at + 4, at + 8);
    const body = buffer.subarray(at + 8, at + 8 + length);
    if (type === 'IHDR') {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      const depth = body[8];
      const color = body[9];
      if (depth !== 8 || (color !== 2 && color !== 6) || body[12] !== 0) {
        throw new Error(`unsupported PNG: depth ${depth}, colour ${color}, interlace ${body[12]}`);
      }
      channels = color === 6 ? 4 : 3;
    } else if (type === 'IDAT') {
      parts.push(body);
    }
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(parts));
  const stride = width * channels;
  const out = new Uint8Array(width * height * 4);
  const rows: Uint8Array[] = [];
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Uint8Array.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    const above = rows[y - 1];
    for (let x = 0; x < stride; x++) {
      const left = x >= channels ? (line[x - channels] ?? 0) : 0;
      const up = above?.[x] ?? 0;
      const upLeft = x >= channels ? (above?.[x - channels] ?? 0) : 0;
      let add = 0;
      if (filter === 1) add = left;
      else if (filter === 2) add = up;
      else if (filter === 3) add = (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        add = pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
      }
      line[x] = ((line[x] ?? 0) + add) & 255;
    }
    rows.push(line);
    for (let x = 0; x < width; x++) {
      const from = x * channels;
      const to = (y * width + x) * 4;
      out[to] = line[from] ?? 0;
      out[to + 1] = line[from + 1] ?? 0;
      out[to + 2] = line[from + 2] ?? 0;
      out[to + 3] = channels === 4 ? (line[from + 3] ?? 255) : 255;
    }
  }
  return { width, height, data: out };
}

/** The number of pixels that differ from the most common colour by more than `limit`. */
export function inkPixels(image: Image, limit = 48): number {
  const { data } = image;
  const counts = new Map<number, number>();
  for (let at = 0; at < data.length; at += 4) {
    const colour = ((data[at] ?? 0) << 16) | ((data[at + 1] ?? 0) << 8) | (data[at + 2] ?? 0);
    counts.set(colour, (counts.get(colour) ?? 0) + 1);
  }
  let background = 0;
  let most = -1;
  for (const [colour, count] of counts) {
    if (count > most) {
      most = count;
      background = colour;
    }
  }
  const [red, green, blue] = [(background >> 16) & 255, (background >> 8) & 255, background & 255];
  let ink = 0;
  for (let at = 0; at < data.length; at += 4) {
    const diff =
      Math.abs((data[at] ?? 0) - red) +
      Math.abs((data[at + 1] ?? 0) - green) +
      Math.abs((data[at + 2] ?? 0) - blue);
    if (diff > limit) ink++;
  }
  return ink;
}
