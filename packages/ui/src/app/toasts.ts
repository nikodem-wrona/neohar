// The toasts of a window: short messages in the bottom right corner that say what became of an
// action, such as an answer that arrived. They are not failures: a failure of a command shows in
// the banner (`app/failures.ts`). A toast goes after `TOAST_SHOW_MS`, or when the person closes it.
// While the pointer is on the toasts, none goes.

import { realTimers, type Timers } from '../lib/timing';
import { createValue, type Source } from '../lib/value';

export type ToastTone = 'ok' | 'warn' | 'error';

/** What a toast says. */
export interface ToastText {
  title: string;
  description?: string;
  tone: ToastTone;
}

export interface Toast extends ToastText {
  /** One more for each toast. */
  id: number;
}

/** How long a toast shows. */
export const TOAST_SHOW_MS = 5_000;

/** How many toasts show at a time. A new toast pushes out the oldest. */
export const TOAST_LIMIT = 3;

export interface Toasts {
  /** The toasts that show, the oldest first. */
  shown: Source<Toast[]>;
  show(toast: ToastText): void;
  dismiss(id: number): void;
  /** Stops the time of every toast, while the pointer is on them. */
  pause(): void;
  /** Starts the time of every toast again, from the start. */
  resume(): void;
  /** Removes every toast. */
  clear(): void;
}

export function createToasts(timers: Timers = realTimers): Toasts {
  const shown = createValue<Toast[]>([]);
  const drop = (id: number): void => {
    shown.set(shown.get().filter((toast) => toast.id !== id));
  };
  const handles = new Map<number, unknown>();
  let count = 0;
  let paused = false;

  const stop = (id: number): void => {
    if (handles.has(id)) timers.clear(handles.get(id));
    handles.delete(id);
  };
  const start = (id: number): void => {
    stop(id);
    handles.set(
      id,
      timers.set(() => {
        handles.delete(id);
        drop(id);
      }, TOAST_SHOW_MS),
    );
  };
  const dismiss = (id: number): void => {
    stop(id);
    drop(id);
  };

  return {
    shown,
    show: (text) => {
      count += 1;
      const toast = { id: count, ...text };
      const kept = [...shown.get(), toast];
      for (const old of kept.slice(0, -TOAST_LIMIT)) stop(old.id);
      shown.set(kept.slice(-TOAST_LIMIT));
      if (!paused) start(toast.id);
    },
    dismiss,
    pause: () => {
      paused = true;
      for (const id of [...handles.keys()]) stop(id);
    },
    resume: () => {
      paused = false;
      for (const toast of shown.get()) start(toast.id);
    },
    clear: () => {
      for (const id of [...handles.keys()]) stop(id);
      shown.set([]);
    },
  };
}
