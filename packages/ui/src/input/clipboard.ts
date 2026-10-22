// The chords of copy and paste in a terminal surface. On macOS they are ⌘C and
// ⌘V. On other systems Ctrl+C must reach the program (it is the interrupt), so they are
// Ctrl+Shift+C and Ctrl+Shift+V.

import type { ChordEventLike } from './reserved';

export type Platform = 'macos' | 'other';

function chord(e: ChordEventLike, code: string, platform: Platform): boolean {
  if (e.code !== code || e.altKey) return false;
  return platform === 'macos'
    ? e.metaKey && !e.ctrlKey && !e.shiftKey
    : e.ctrlKey && e.shiftKey && !e.metaKey;
}

/** Copies the selection of the pane. The key never goes to the program. */
export function isCopyChord(e: ChordEventLike, platform: Platform): boolean {
  return chord(e, 'KeyC', platform);
}

/**
 * Pastes. The page lets this key reach the browser, which makes a `paste` event with the text. The
 * key never goes to the program.
 */
export function isPasteChord(e: ChordEventLike, platform: Platform): boolean {
  return chord(e, 'KeyV', platform);
}

/**
 * Writes text to the clipboard of a phone in the tap that asked for it. The text comes
 * from the core later, so the write takes a promise: a `ClipboardItem` keeps the gesture of the tap
 * while it waits **[verify]**. A browser without `ClipboardItem` writes the text when it is there.
 */
export function writeClipboard(text: Promise<string>): Promise<void> {
  const clipboard = globalThis.navigator?.clipboard;
  if (!clipboard) return Promise.reject(new Error('this page has no clipboard'));
  if (typeof ClipboardItem !== 'undefined' && typeof clipboard.write === 'function') {
    const blob = text.then((value) => new Blob([value], { type: 'text/plain' }));
    return clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
  }
  return text.then((value) => clipboard.writeText(value));
}

/** The text of the clipboard of a phone, for Paste of the key bar. iOS asks the person first. */
export function readClipboard(): Promise<string> {
  const clipboard = globalThis.navigator?.clipboard;
  if (!clipboard?.readText) return Promise.reject(new Error('this page cannot read the clipboard'));
  return clipboard.readText();
}
