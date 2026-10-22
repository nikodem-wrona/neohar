import { expect, test } from 'bun:test';
import { AckBook } from './acks';

test('an empty book gives nothing', () => {
  expect(new AckBook().take()).toBeNull();
});

test('the newest revision of a pane is acknowledged once', () => {
  const book = new AckBook();
  book.paint(3, 10);
  book.paint(3, 11);
  book.paint(4, 2);
  expect(book.take()).toEqual({
    acks: [
      { pane: 3, rev: 11 },
      { pane: 4, rev: 2 },
    ],
    need_keyframe: [],
  });
  expect(book.take()).toBeNull();
});

test('a pane that needs a keyframe is listed with the acks', () => {
  const book = new AckBook();
  book.paint(1, 5);
  book.needKeyframe(2);
  book.needKeyframe(2);
  expect(book.take()).toEqual({ acks: [{ pane: 1, rev: 5 }], need_keyframe: [2] });
});

test('a pane that is gone is forgotten', () => {
  const book = new AckBook();
  book.paint(1, 5);
  book.needKeyframe(1);
  book.forget(1);
  expect(book.take()).toBeNull();
});
