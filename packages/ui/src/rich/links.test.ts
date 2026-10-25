import { expect, test } from 'bun:test';
import { clickedLink, isLoopback } from './links';

test('an address of the loopback interface is an address of the Mac', () => {
  expect(isLoopback('http://localhost:5173/app')).toBe(true);
  expect(isLoopback('http://127.0.0.1:8080')).toBe(true);
  expect(isLoopback('http://[::1]:3000')).toBe(true);
  expect(isLoopback('https://example.com')).toBe(false);
  expect(isLoopback('not a url')).toBe(false);
});

test('a click on a link of the core carries the gesture token of the host', () => {
  const link = 'nh://open?url=https%3A%2F%2Fexample.com%2F';
  expect(clickedLink(link, 'a1/b2')).toBe(`${link}&gesture=a1%2Fb2`);
  // Without a token (a browser) or for another link, the click opens nothing.
  expect(clickedLink(link, undefined)).toBeNull();
  expect(clickedLink(link, '')).toBeNull();
  expect(clickedLink('https://example.com/', 'token')).toBeNull();
  expect(clickedLink(`${link}&gesture=other`, 'token')).toBeNull();
});
