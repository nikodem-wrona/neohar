// A layer of shadcn/ui that is open tells the view, so that the pane does not take the focus from
// it (`app/view.ts`, `layers`).

import { useEffect } from 'react';
import { useApp } from './context';

/** Counts the layer as open while `open` is true. */
export function useLayer(open: boolean): void {
  const { view } = useApp();
  useEffect(() => (open ? view.openLayer() : undefined), [open, view]);
}
