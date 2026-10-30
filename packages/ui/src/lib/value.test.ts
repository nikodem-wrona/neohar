import { describe, expect, it } from 'bun:test';
import { createValue } from './value';

describe('a value', () => {
  it('gives the last value that it got', () => {
    const value = createValue(1);
    value.set(2);
    expect(value.get()).toBe(2);
  });

  it('calls each listener after a change, and not after a stop', () => {
    const value = createValue('a');
    const seen: string[] = [];
    const stop = value.subscribe(() => seen.push(value.get()));
    value.set('b');
    stop();
    value.set('c');
    expect(seen).toEqual(['b']);
  });

  it('calls no listener for an equal value', () => {
    const value = createValue({ n: 1 }, (a, b) => a.n === b.n);
    let calls = 0;
    value.subscribe(() => (calls += 1));
    value.set({ n: 1 });
    value.set({ n: 2 });
    expect(calls).toBe(1);
  });

  it('calls every listener once, also when a listener stops another one', () => {
    const value = createValue(0);
    const calls: string[] = [];
    const stops: (() => void)[] = [];
    stops.push(
      value.subscribe(() => {
        calls.push('first');
        stops[1]?.();
      }),
    );
    stops.push(value.subscribe(() => calls.push('second')));
    value.set(1);
    value.set(2);
    expect(calls).toEqual(['first', 'second', 'first']);
  });
});
