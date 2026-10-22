// A small TOML writer for the case files. It writes only what the cases need: strings, numbers,
// booleans, arrays and one line inline tables.

export type Value = string | number | boolean | Value[] | { [key: string]: Value | undefined };

function quote(text: string): string {
  let out = '"';
  for (const c of text) {
    const code = c.codePointAt(0) ?? 0;
    if (c === '"') out += '\\"';
    else if (c === '\\') out += '\\\\';
    else if (code < 0x20 || code === 0x7f) out += `\\u${code.toString(16).padStart(4, '0')}`;
    else out += c;
  }
  return `${out}"`;
}

/** One value in the one line form. */
export function inline(value: Value): string {
  if (typeof value === 'string') return quote(value);
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (Array.isArray(value)) return `[${value.map(inline).join(', ')}]`;
  const fields = Object.entries(value).filter(([, v]) => v !== undefined);
  return `{ ${fields.map(([k, v]) => `${k} = ${inline(v as Value)}`).join(', ')} }`;
}

/** A table whose fields are written one for each line. Array fields of tables get a line for each item. */
export function table(name: string, fields: Record<string, Value | undefined>): string {
  const lines = [`[[${name}]]`];
  for (const [key, value] of Object.entries(fields)) {
    if (value === undefined) continue;
    if (
      Array.isArray(value) &&
      value.length > 1 &&
      value.every((v) => typeof v === 'object' && !Array.isArray(v))
    ) {
      lines.push(`${key} = [`, ...value.map((v) => `  ${inline(v)},`), ']');
    } else {
      lines.push(`${key} = ${inline(value)}`);
    }
  }
  return lines.join('\n');
}
