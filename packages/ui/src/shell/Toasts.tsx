// The toasts of the window in the bottom right corner: the Toaster of shadcn/ui (Sonner) shows the
// toasts of `app/toasts.ts`. The model keeps the time of each toast and the limit, so a toast goes
// when the model drops it, and the close button of a toast drops it from the model. While the
// pointer is on the toasts, none goes. On the desk they lie over the status bar. On a phone they lie
// over the pane, above the bottom bar, from edge to edge.

import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { TOAST_LIMIT, type Toast, type Toasts as Model } from '../app/toasts';
import { Toaster } from '../components/ui/sonner';
import { useValue } from '../lib/hooks';

/** The kind of toast of Sonner for each tone. */
const KIND = { ok: toast.success, warn: toast.warning, error: toast.error } as const;

export function Toasts(props: { toasts: Model }) {
  const shown = useValue(props.toasts.shown);
  const showing = useRef(new Map<number, Toast>());
  const { toasts } = props;

  useEffect(() => {
    const now = new Set(shown.map((item) => item.id));
    for (const id of showing.current.keys()) {
      if (now.has(id)) continue;
      showing.current.delete(id);
      toast.dismiss(id);
    }
    for (const item of shown) {
      if (showing.current.has(item.id)) continue;
      showing.current.set(item.id, item);
      KIND[item.tone](item.title, {
        id: item.id,
        ...(item.description ? { description: item.description } : {}),
        // The model takes the toast away.
        duration: Number.POSITIVE_INFINITY,
        onDismiss: () => toasts.dismiss(item.id),
      });
    }
  }, [shown, toasts]);

  return (
    // The box takes no place in the grid of the shell: it is fixed, and so is the list of Sonner.
    <div
      className="fixed"
      onPointerEnter={() => toasts.pause()}
      onPointerLeave={() => toasts.resume()}
    >
      <Toaster
        position="bottom-right"
        closeButton
        visibleToasts={TOAST_LIMIT}
        offset={{
          right: 'calc(var(--gutter) + var(--s3))',
          bottom: 'calc(var(--bar) + 2 * var(--gutter) + var(--s2))',
        }}
        mobileOffset={{
          left: 'max(var(--gutter), env(safe-area-inset-left))',
          right: 'max(var(--gutter), env(safe-area-inset-right))',
          bottom:
            'calc(max(var(--gutter), env(safe-area-inset-bottom)) + var(--control) + var(--s2))',
        }}
      />
    </div>
  );
}
