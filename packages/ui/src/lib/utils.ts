// Small helpers of the components.

/** A part of a class list: a name, nothing, or names that are on when their value is true. */
export type ClassPart = string | false | null | undefined | Record<string, boolean | undefined>;

/** The class names of the parts, in their order, separated by spaces. */
export function cn(...parts: ClassPart[]): string {
  const names: string[] = [];
  for (const part of parts) {
    if (!part) continue;
    if (typeof part === 'string') {
      names.push(part);
      continue;
    }
    for (const [name, on] of Object.entries(part)) if (on) names.push(name);
  }
  return names.join(' ');
}
