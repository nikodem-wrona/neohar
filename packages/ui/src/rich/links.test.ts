import { expect, test } from 'bun:test';
import { isLoopback } from './links';

test('an address of the loopback interface is an address of the Mac', () => {
  expect(isLoopback('http://localhost:5173/app')).toBe(true);
  expect(isLoopback('http://127.0.0.1:8080')).toBe(true);
  expect(isLoopback('http://[::1]:3000')).toBe(true);
  expect(isLoopback('https://example.com')).toBe(false);
  expect(isLoopback('not a url')).toBe(false);
});
