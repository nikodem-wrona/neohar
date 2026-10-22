import { expect, test } from 'bun:test';
import { Keyboard, compositionText, usCharacter, type KeyEventLike } from './keyboard';

/** A key event with the fields of a browser event. */
function event(fields: Partial<KeyEventLike> & { states?: string[] } = {}): KeyEventLike {
  const { states = [], ...rest } = fields;
  return {
    type: 'keydown',
    key: 'a',
    code: 'KeyA',
    location: 0,
    repeat: false,
    isComposing: false,
    keyCode: 65,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    getModifierState: (name) => states.includes(name),
    ...rest,
  };
}

const other = () => new Keyboard({ platform: 'other', optionAsAlt: 'none' });
const mac = (optionAsAlt: 'none' | 'left' | 'right' | 'both') =>
  new Keyboard({ platform: 'macos', optionAsAlt });

test('a plain key is a press with its text', () => {
  expect(other().toLogical(event())).toEqual({
    type: 'key',
    code: 'KeyA',
    key: 'a',
    mods: [],
    kind: 'press',
    text: 'a',
    location: 0,
  });
});

test('press, repeat and release are told apart', () => {
  const k = other();
  expect(k.toLogical(event())?.kind).toBe('press');
  expect(k.toLogical(event({ repeat: true }))?.kind).toBe('repeat');
  const release = k.toLogical(event({ type: 'keyup' }));
  expect(release?.kind).toBe('release');
  expect(release?.text).toBeUndefined();
});

test('modifiers keep the order shift, ctrl, alt, meta, locks', () => {
  const logical = other().toLogical(
    event({
      shiftKey: true,
      ctrlKey: true,
      altKey: true,
      metaKey: true,
      states: ['NumLock', 'CapsLock'],
    }),
  );
  expect(logical?.mods).toEqual(['shift', 'ctrl', 'alt', 'meta', 'caps_lock', 'num_lock']);
});

test('Ctrl and Meta chords carry no text', () => {
  expect(other().toLogical(event({ ctrlKey: true }))?.text).toBeUndefined();
  expect(other().toLogical(event({ metaKey: true }))?.text).toBeUndefined();
  expect(other().toLogical(event({ altKey: true }))?.text).toBe('a');
});

test('named keys carry no text', () => {
  const logical = other().toLogical(event({ key: 'Enter', code: 'Enter', keyCode: 13 }));
  expect(logical?.text).toBeUndefined();
  expect(logical?.key).toBe('Enter');
});

test('an input method owns the key while it composes', () => {
  const k = other();
  expect(k.toLogical(event({ isComposing: true }))).toBeNull();
  expect(k.toLogical(event({ keyCode: 229 }))).toBeNull();
  expect(k.toLogical(event({ type: 'keypress' }))).toBeNull();
  expect(compositionText({ data: '日本語' })).toEqual({ type: 'text', text: '日本語' });
  expect(compositionText({ data: '' })).toBeNull();
});

test('a dead key sends nothing', () => {
  expect(other().toLogical(event({ key: 'Dead', code: 'Quote' }))).toBeNull();
  expect(other().toLogical(event({ key: 'Dead', code: 'Quote', type: 'keyup' }))).toBeNull();
});

test('AltGr makes a character: Ctrl and Alt are cleared', () => {
  const windows = other().toLogical(
    event({ key: '@', code: 'KeyQ', ctrlKey: true, altKey: true, states: ['AltGraph'] }),
  );
  expect(windows?.mods).toEqual([]);
  expect(windows?.text).toBe('@');
  const linux = other().toLogical(event({ key: '{', code: 'Digit7', states: ['AltGraph'] }));
  expect(linux?.mods).toEqual([]);
  expect(linux?.key).toBe('{');
  // The AltGr key itself is not a character.
  const key = other().toLogical(
    event({ key: 'AltGraph', code: 'AltRight', location: 2, states: ['AltGraph'], altKey: true }),
  );
  expect(key?.mods).toEqual(['alt']);
});

test('on macOS the Option key makes a character unless it is set to be Alt', () => {
  const composed = event({ key: 'å', code: 'KeyA', altKey: true });
  const k = mac('none');
  expect(k.toLogical(composed)).toMatchObject({ key: 'å', mods: [], text: 'å' });
  // Option+E is a dead key.
  expect(k.toLogical(event({ key: 'Dead', code: 'KeyE', altKey: true }))).toBeNull();
  // Not on Linux: Alt+ö is an Alt chord.
  expect(other().toLogical(event({ key: 'ö', code: 'Semicolon', altKey: true }))).toMatchObject({
    key: 'ö',
    mods: ['alt'],
  });
});

test('Option as Alt takes the base character from the key', () => {
  const k = mac('both');
  expect(k.toLogical(event({ key: 'å', code: 'KeyA', altKey: true }))).toMatchObject({
    key: 'a',
    mods: ['alt'],
    text: 'a',
  });
  expect(
    k.toLogical(event({ key: 'Å', code: 'KeyA', altKey: true, shiftKey: true })),
  ).toMatchObject({ key: 'A', mods: ['shift', 'alt'] });
  expect(k.toLogical(event({ key: 'Dead', code: 'KeyE', altKey: true }))).toMatchObject({
    key: 'e',
    mods: ['alt'],
  });
  expect(k.toLogical(event({ key: '¡', code: 'Digit1', altKey: true }))).toMatchObject({
    key: '1',
  });
  // A key that is not on the table keeps its key. A named key keeps Alt.
  expect(k.toLogical(event({ key: 'ö', code: 'IntlBackslash', altKey: true }))).toMatchObject({
    key: 'ö',
    mods: ['alt'],
  });
  expect(k.toLogical(event({ key: 'ArrowLeft', code: 'ArrowLeft', altKey: true }))).toMatchObject({
    key: 'ArrowLeft',
    mods: ['alt'],
  });
});

test('Option as Alt follows the side of the Option key that is down', () => {
  const left = mac('left');
  const composed = event({ key: 'å', code: 'KeyA', altKey: true });
  left.toLogical(event({ key: 'Alt', code: 'AltRight', location: 2, altKey: true, keyCode: 18 }));
  expect(left.toLogical(composed)).toMatchObject({ key: 'å', mods: [] });
  left.toLogical(event({ type: 'keyup', key: 'Alt', code: 'AltRight', location: 2, keyCode: 18 }));
  left.toLogical(event({ key: 'Alt', code: 'AltLeft', location: 1, altKey: true, keyCode: 18 }));
  expect(left.toLogical(composed)).toMatchObject({ key: 'a', mods: ['alt'] });
  left.reset();
  expect(left.toLogical(composed)).toMatchObject({ key: 'å', mods: [] });
});

test('the US table', () => {
  expect(usCharacter('KeyQ', false)).toBe('q');
  expect(usCharacter('KeyQ', true)).toBe('Q');
  expect(usCharacter('Digit2', true)).toBe('@');
  expect(usCharacter('Slash', true)).toBe('?');
  expect(usCharacter('Enter', false)).toBeUndefined();
  expect(usCharacter('IntlBackslash', false)).toBeUndefined();
});

test('a dead key with Option as Alt is the Alt chord, although the browser reports 229', () => {
  const dead = event({ key: 'Dead', code: 'KeyE', altKey: true, keyCode: 229 });
  expect(mac('both').toLogical(dead)).toMatchObject({ key: 'e', mods: ['alt'] });
  expect(mac('none').toLogical(dead)).toBeNull();
  // An input method that composes still owns the key.
  expect(mac('both').toLogical({ ...dead, isComposing: true })).toBeNull();
});
