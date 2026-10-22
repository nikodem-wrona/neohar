// The files that the person gives to a task in the dialog: from "Add files…" or from a
// paste, such as a screenshot on the clipboard. The page reads their bytes and sends them as
// base64 in `uploads`. It does not look into them: the core names their type and keeps them.

import type { PendingFile } from './model';

let next = 0;

/** The bytes of a file as base64. */
function base64Of(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      resolve(url.slice(url.indexOf(',') + 1));
    };
    reader.onerror = () => reject(reader.error ?? new Error('the file did not read'));
    reader.readAsDataURL(file);
  });
}

/** Reads files for the form. A pasted image without a name is `pasted.png`. */
export async function pendingFiles(files: Iterable<File>): Promise<PendingFile[]> {
  const out: PendingFile[] = [];
  for (const file of files) {
    next += 1;
    const extension = file.type.startsWith('image/') ? file.type.slice(6).split('+')[0] : 'bin';
    out.push({
      key: `pending-${next}`,
      name: file.name || `pasted.${extension}`,
      bytes: file.size,
      data: await base64Of(file),
    });
  }
  return out;
}

/** The files of a paste, or none when the paste is text. */
export function filesOfPaste(event: ClipboardEvent): File[] {
  return Array.from(event.clipboardData?.files ?? []);
}
