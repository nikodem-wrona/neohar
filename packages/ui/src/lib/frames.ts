// Sends at most one value for each animation frame. A drag of a floating pane makes many
// pointer events for each frame. The sender keeps the last value and sends it when the frame
// comes, so the core gets one `pane.set_float_rect` for each frame. The page gives the frame
// function, so that a test can run frames by hand.

export interface FrameSender<T> {
  /** Keeps the value. The frame sends the last value that was kept. */
  push(value: T): void;
  /** Sends the kept value now, if there is one. A release of the pointer calls it. */
  flush(): void;
  /** Drops the kept value. */
  cancel(): void;
}

export interface Frames {
  request(callback: () => void): number;
  cancel(id: number): void;
}

/** The frames of the page. */
export const pageFrames: Frames = {
  request: (callback) => requestAnimationFrame(callback),
  cancel: (id) => cancelAnimationFrame(id),
};

export function createFrameSender<T>(send: (value: T) => void, frames: Frames): FrameSender<T> {
  let kept: { value: T } | null = null;
  let pending: number | null = null;
  const run = (): void => {
    pending = null;
    if (kept === null) return;
    const { value } = kept;
    kept = null;
    send(value);
  };
  return {
    push(value) {
      kept = { value };
      pending ??= frames.request(run);
    },
    flush() {
      if (pending !== null) {
        frames.cancel(pending);
      }
      run();
    },
    cancel() {
      kept = null;
      if (pending !== null) frames.cancel(pending);
      pending = null;
    },
  };
}
