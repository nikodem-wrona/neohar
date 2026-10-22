// The drag of a split divider (the one local prediction that the UI makes). While the
// pointer is down, the page shows the new shares without asking the core. On release it sends one
// `layout.set_ratios`, and the projection gives the final shares.

/** The smallest share of a child while the pointer drags. The core accepts 0.02. */
export const MIN_SHARE = 0.05;

/**
 * The shares after the divider behind child `index` moves by `delta`, as a fraction of the split.
 * Only the two children at the divider change. Each keeps at least [`MIN_SHARE`], and the sum of
 * the two stays as it was.
 */
export function dragRatios(ratios: readonly number[], index: number, delta: number): number[] {
  const before = ratios[index];
  const after = ratios[index + 1];
  if (before === undefined || after === undefined) return [...ratios];
  const pair = before + after;
  const low = Math.min(MIN_SHARE, pair / 2);
  const moved = Math.min(Math.max(before + delta, low), pair - low);
  const next = [...ratios];
  next[index] = moved;
  next[index + 1] = pair - moved;
  return next;
}

/** Whether two lists of shares differ by more than the core would notice. */
export function ratiosDiffer(a: readonly number[], b: readonly number[]): boolean {
  return a.length !== b.length || a.some((share, i) => Math.abs(share - (b[i] ?? 0)) > 1e-4);
}
