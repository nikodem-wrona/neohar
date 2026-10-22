// `mise run check:holder-size`: the holder stays small enough to audit.
// Counts the lines of code in crates/nh-hold/src and fails above LIMIT. A line counts when it has
// code on it. Blank lines, comment-only lines and the bodies of inline `#[cfg(test)] mod x { … }`
// modules do not count. A `#[cfg(test)] mod x;` that points to another file is an error, so that
// nobody hides code from the count by accident: write test modules inline.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

export const LIMIT = 2500;

/**
 * Replaces comments with spaces and the content of string and character literals with `_`,
 * so that text matching and brace counting see code only. Line breaks stay in place.
 */
export function blankNonCode(src: string): string {
  const out: string[] = [];
  const blank = (text: string, fill: string) => text.replace(/[^\n]/g, fill);
  let i = 0;
  while (i < src.length) {
    const rest = src.slice(i);
    const two = src.slice(i, i + 2);
    const raw = /^b?r(#*)"/.exec(rest);
    const prev = i > 0 ? src.charAt(i - 1) : '';
    if (two === '//') {
      const end = src.indexOf('\n', i);
      const stop = end === -1 ? src.length : end;
      out.push(blank(src.slice(i, stop), ' '));
      i = stop;
    } else if (two === '/*') {
      let depth = 1;
      let j = i + 2;
      while (j < src.length && depth > 0) {
        if (src.startsWith('/*', j)) {
          depth++;
          j += 2;
        } else if (src.startsWith('*/', j)) {
          depth--;
          j += 2;
        } else {
          j++;
        }
      }
      out.push(blank(src.slice(i, j), ' '));
      i = j;
    } else if (raw && !/[A-Za-z0-9_]/.test(prev)) {
      const close = `"${raw[1]}`;
      const end = src.indexOf(close, i + raw[0].length);
      const stop = end === -1 ? src.length : end + close.length;
      out.push(blank(src.slice(i, stop), '_'));
      i = stop;
    } else if (src[i] === '"') {
      let j = i + 1;
      while (j < src.length && src[j] !== '"') j += src[j] === '\\' ? 2 : 1;
      out.push(blank(src.slice(i, j + 1), '_'));
      i = j + 1;
    } else if (src[i] === "'") {
      // A character literal ('x', '\n', '\u{1F600}') or a lifetime ('a).
      const escaped = src[i + 1] === '\\';
      const cp = String.fromCodePoint(src.codePointAt(i + 1) ?? 32);
      const end = escaped
        ? src.indexOf("'", i + 3)
        : src[i + 1 + cp.length] === "'"
          ? i + 1 + cp.length
          : -1;
      if (end === -1) {
        out.push("'");
        i++;
      } else {
        out.push(blank(src.slice(i, end + 1), '_'));
        i = end + 1;
      }
    } else {
      out.push(src.charAt(i));
      i++;
    }
  }
  return out.join('');
}

const TEST_MOD =
  /#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*([{;])/g;

/** The number of lines of code in one Rust source file; see the header comment. */
export function countCodeLines(src: string): number {
  const code = blankNonCode(src);
  const lines = code.split('\n');
  const skip = new Set<number>();
  for (const m of code.matchAll(TEST_MOD)) {
    if (m[1] === ';') throw new Error('write `#[cfg(test)]` modules inline, not as separate files');
    const first = code.slice(0, m.index).split('\n').length - 1;
    let depth = 0;
    let last = first + m[0].split('\n').length - 1;
    for (let j = m.index + m[0].length - 1; j < code.length; j++) {
      if (code[j] === '\n') last++;
      else if (code[j] === '{') depth++;
      else if (code[j] === '}' && --depth === 0) break;
    }
    for (let n = first; n <= last; n++) skip.add(n);
  }
  return lines.filter((line, n) => !skip.has(n) && line.trim() !== '').length;
}

function rustFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? rustFiles(join(dir, entry.name))
      : entry.name.endsWith('.rs')
        ? [join(dir, entry.name)]
        : [],
  );
}

if (import.meta.main) {
  const dir = join(import.meta.dir, '..', 'crates', 'nh-hold', 'src');
  const total = rustFiles(dir).reduce(
    (sum, file) => sum + countCodeLines(readFileSync(file, 'utf8')),
    0,
  );
  console.log(`nh-hold: ${total} lines of code (limit ${LIMIT})`);
  if (total > LIMIT) {
    console.error('crates/nh-hold/src is over its line limit. Move logic to another crate.');
    process.exit(1);
  }
}
