// Text transforms for generated pages: HTML safety and paragraphs.

/** Splits text into code spans and the text between them. Code spans keep their backticks. */
function splitCode(text: string): { code: boolean; text: string }[] {
  const parts: { code: boolean; text: string }[] = [];
  const re = /(`+)([\s\S]*?)\1/g;
  let last = 0;
  for (const match of text.matchAll(re)) {
    const start = match.index ?? 0;
    if (start > last) parts.push({ code: false, text: text.slice(last, start) });
    parts.push({ code: true, text: match[0] });
    last = start + match[0].length;
  }
  if (last < text.length) parts.push({ code: false, text: text.slice(last) });
  return parts;
}

/** Makes prose from the code safe: a `<` before a letter outside code becomes `&lt;`, so CommonMark does not read a tag. */
export function prose(text: string): string {
  return splitCode(text)
    .map((part) => (part.code ? part.text : part.text.replace(/<(?=[A-Za-z/!])/g, '&lt;')))
    .join('');
}

/** Joins the lines of each paragraph of a doc comment. */
export function paragraphs(text: string | undefined): string {
  if (!text) return '';
  return text
    .split(/\n\s*\n/)
    .map((p) => p.replace(/\s*\n\s*/g, ' ').trim())
    .filter(Boolean)
    .join('\n\n');
}
