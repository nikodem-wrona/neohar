import { describe, expect, it } from 'bun:test';
import { misses, percentile, summarize, timesOf } from './shim-bench';

describe('percentile', () => {
  it('takes the nearest rank', () => {
    const hundred = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(hundred, 0.5)).toBe(50);
    expect(percentile(hundred, 0.99)).toBe(99);
    expect(percentile(hundred, 1)).toBe(100);
    expect(percentile(hundred, 0)).toBe(1);
  });

  it('does not round the rank down for a short list', () => {
    expect(percentile([1, 2, 3], 0.99)).toBe(3);
    expect(percentile([7], 0.5)).toBe(7);
  });

  it('refuses an empty list', () => {
    expect(() => percentile([], 0.5)).toThrow();
  });
});

describe('summarize', () => {
  it('sorts the times and converts seconds to milliseconds', () => {
    const times = [0.004, 0.002, 0.003, 0.02, 0.001];
    const s = summarize(times);
    expect(s.runs).toBe(5);
    expect(s.min).toBeCloseTo(1);
    expect(s.p50).toBeCloseTo(3);
    expect(s.p99).toBeCloseTo(20);
    expect(s.max).toBeCloseTo(20);
  });
});

describe('misses', () => {
  const summary = { runs: 500, min: 1, p50: 4.2, p99: 16, max: 30 };

  it('names each budget that the run misses', () => {
    expect(misses(summary, { p50: 5, p99: 15 })).toEqual(['p99 16.00 ms > 15 ms']);
    expect(misses({ ...summary, p50: 6 }, { p50: 5, p99: 20 })).toEqual(['p50 6.00 ms > 5 ms']);
  });

  it('checks p50 only when there is no p99 budget', () => {
    expect(misses(summary, { p50: 5 })).toEqual([]);
  });

  it('accepts a run on the budget', () => {
    expect(misses({ ...summary, p50: 5, p99: 15 }, { p50: 5, p99: 15 })).toEqual([]);
  });
});

describe('timesOf', () => {
  it('reads the times of the first result', () => {
    expect(timesOf('{"results":[{"times":[0.001,0.002]}]}')).toEqual([0.001, 0.002]);
  });

  it('refuses an export without times', () => {
    expect(() => timesOf('{"results":[]}')).toThrow();
    expect(() => timesOf('{"results":[{"times":[]}]}')).toThrow();
  });
});
