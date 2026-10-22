import { describe, expect, test } from 'bun:test';
import {
  BACKSPACE,
  chordOf,
  codeOf,
  endsWord,
  FieldText,
  key,
  nextSticky,
  stickyMods,
  withMods,
} from './soft';

const text = (value: string) => ({ type: 'text' as const, text: value });

describe('the field of a pane', () => {
  test('a typed character goes as text', () => {
    const field = new FieldText();
    expect(field.change('h')).toEqual([text('h')]);
    expect(field.change('he')).toEqual([text('e')]);
    expect(field.text).toBe('he');
  });

  test('a correction deletes the characters that changed and types the new ones', () => {
    const field = new FieldText();
    field.change('teh');
    // iOS replaces the word: insertReplacementText.
    expect(field.change('the')).toEqual([BACKSPACE, BACKSPACE, text('he')]);
  });

  test('a word of the prediction bar and dictation add text after what is there', () => {
    const field = new FieldText();
    field.change('hel');
    expect(field.change('hello ')).toEqual([text('lo ')]);
    field.clear();
    expect(field.change('Run the tests.')).toEqual([text('Run the tests.')]);
  });

  test('a Backspace in the field deletes one character', () => {
    const field = new FieldText();
    field.change('abc');
    expect(field.change('ab')).toEqual([BACKSPACE]);
  });

  test('the count is in characters, not in UTF-16 units', () => {
    const field = new FieldText();
    field.change('ok 👍');
    // The emoji is two UTF-16 units and one character: one Backspace.
    expect(field.change('ok ')).toEqual([BACKSPACE]);
    expect(field.change('ok 🎉')).toEqual([text('🎉')]);
  });

  test('a space or a line break ends the word, and the field starts again', () => {
    expect(endsWord('hello ')).toBe(true);
    expect(endsWord('hello\n')).toBe(true);
    expect(endsWord('hello')).toBe(false);
  });
});

describe('the key bar', () => {
  test('Ctrl and Alt are for the next key, and a second tap locks them', () => {
    expect(nextSticky('off')).toBe('once');
    expect(nextSticky('once')).toBe('locked');
    expect(nextSticky('locked')).toBe('off');
    expect(stickyMods('once', 'off')).toEqual(['ctrl']);
    expect(stickyMods('locked', 'once')).toEqual(['ctrl', 'alt']);
  });

  test('a character with Ctrl is the key of the US layout with no text', () => {
    expect(chordOf('c', ['ctrl'])).toEqual({
      type: 'key',
      code: 'KeyC',
      key: 'c',
      mods: ['ctrl'],
      kind: 'press',
      location: 0,
    });
    // Alt keeps the text, and a shifted character has Shift.
    expect(chordOf('b', ['alt'])).toMatchObject({ code: 'KeyB', mods: ['alt'], text: 'b' });
    expect(chordOf('|', ['ctrl'])).toMatchObject({ code: 'Backslash', mods: ['shift', 'ctrl'] });
    // A character with no key of the layout goes as text.
    expect(chordOf('é', ['ctrl'])).toEqual(text('é'));
  });

  test('the codes of the characters of the layout', () => {
    expect(codeOf('a')).toBe('KeyA');
    expect(codeOf('Z')).toBe('KeyZ');
    expect(codeOf('7')).toBe('Digit7');
    expect(codeOf('~')).toBe('Backquote');
    expect(codeOf('/')).toBe('Slash');
    expect(codeOf(' ')).toBe('Space');
    expect(codeOf('ß')).toBeNull();
  });

  test('a key of the bar takes the modifiers that are on, in their order', () => {
    expect(withMods(key('ArrowLeft'), ['alt'])).toMatchObject({ mods: ['alt'] });
    expect(withMods(key('Tab', 'Tab', ['shift']), ['ctrl'])).toMatchObject({
      mods: ['shift', 'ctrl'],
    });
    expect(withMods(key('Escape'), [])).toEqual(key('Escape'));
  });
});
