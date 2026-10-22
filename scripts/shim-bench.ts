// The measurement of the hook shim: the start time of `nh hook claude Stop`.
//
// Runs hyperfine over the shim with the payload file as stdin and the environment of a pane, then
// computes p50 and p99 from the exported times and compares them with the budget. The shim is
// called through a path the caller gives; the budget is for the `~/.local/bin/nh` symlink.
//
// Usage: bun scripts/shim-bench.ts --nh <path> --socket <path> --payload <file>
//          [--runs 500] [--warmup 20] [--p50 5] [--p99 15] [--out shim.json]
//   --socket   the api.sock of a running core, or a path where none listens ("no core" run).
//   --p99      leave it out to check p50 only (the budget of the run without a core).
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

export type Summary = { runs: number; min: number; p50: number; p99: number; max: number };

/** The value at nearest rank `p` (0 to 1) of an ascending list. */
export function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) throw new Error('no samples');
  const rank = Math.max(1, Math.ceil(p * sorted.length));
  return sorted[Math.min(rank, sorted.length) - 1] as number;
}

/** Summarizes the run times, in seconds, as milliseconds. */
export function summarize(seconds: number[]): Summary {
  const ms = seconds.map((s) => s * 1000).sort((a, b) => a - b);
  return {
    runs: ms.length,
    min: percentile(ms, 0),
    p50: percentile(ms, 0.5),
    p99: percentile(ms, 0.99),
    max: percentile(ms, 1),
  };
}

/** What a run misses, as text. Empty means that it met its budget. */
export function misses(summary: Summary, budget: { p50: number; p99?: number }): string[] {
  const out: string[] = [];
  if (summary.p50 > budget.p50) out.push(`p50 ${summary.p50.toFixed(2)} ms > ${budget.p50} ms`);
  if (budget.p99 !== undefined && summary.p99 > budget.p99) {
    out.push(`p99 ${summary.p99.toFixed(2)} ms > ${budget.p99} ms`);
  }
  return out;
}

/** The times of the first command in the JSON that `hyperfine --export-json` writes. */
export function timesOf(exported: string): number[] {
  const data = JSON.parse(exported) as { results?: { times?: number[] }[] };
  const times = data.results?.[0]?.times;
  if (!times || times.length === 0) throw new Error('hyperfine exported no times');
  return times;
}

if (import.meta.main) {
  const { values } = parseArgs({
    options: {
      nh: { type: 'string' },
      socket: { type: 'string' },
      payload: { type: 'string' },
      runs: { type: 'string', default: '500' },
      warmup: { type: 'string', default: '20' },
      p50: { type: 'string', default: '5' },
      p99: { type: 'string' },
      out: { type: 'string', default: 'shim.json' },
    },
  });
  if (!values.nh || !values.socket || !values.payload) {
    console.error('usage: bun scripts/shim-bench.ts --nh <path> --socket <path> --payload <file>');
    process.exit(2);
  }
  const out = values.out ?? 'shim.json';
  const result = spawnSync(
    'hyperfine',
    [
      '-N',
      '--warmup',
      values.warmup ?? '20',
      '--runs',
      values.runs ?? '500',
      '--input',
      values.payload,
      '--export-json',
      out,
      '--style',
      'basic',
      `${values.nh} hook claude Stop`,
    ],
    {
      stdio: ['ignore', 'inherit', 'inherit'],
      env: {
        ...process.env,
        NEOHAR: '1',
        NEOHAR_SOCKET: values.socket,
        NEOHAR_PANE_TOKEN: 'bench-token',
      },
    },
  );
  if (result.status !== 0) {
    console.error(`hyperfine failed with status ${result.status}`);
    process.exit(1);
  }
  const summary = summarize(timesOf(readFileSync(out, 'utf8')));
  const budget = {
    p50: Number(values.p50),
    ...(values.p99 !== undefined ? { p99: Number(values.p99) } : {}),
  };
  const failed = misses(summary, budget);
  writeFileSync(
    out.replace(/\.json$/, '.summary.json'),
    JSON.stringify({ summary, budget, failed }),
  );
  console.log(
    `shim: ${summary.runs} runs, min ${summary.min.toFixed(2)} ms, p50 ${summary.p50.toFixed(2)} ms, ` +
      `p99 ${summary.p99.toFixed(2)} ms, max ${summary.max.toFixed(2)} ms`,
  );
  if (failed.length > 0) {
    console.error(`shim over budget: ${failed.join('; ')}`);
    process.exit(1);
  }
  console.log('shim within budget');
}
