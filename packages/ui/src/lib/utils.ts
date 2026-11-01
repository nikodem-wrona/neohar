// Small helpers of the components. `cn` is the one of shadcn/ui: the class names of the parts, and
// of two Tailwind classes that set the same property, the last one.

import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
