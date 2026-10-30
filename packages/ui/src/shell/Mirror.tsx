// The text mirror for screen readers. An element that is off the screen holds the
// text of the visible rows of the focused pane. The core sends the rows at most 4 times a second,
// and only for a pane that someone follows, so the page subscribes for the focused pane only. The
// last row that has text is a live region, so a screen reader speaks new output.

import { useApp } from '../app/context';
import { useSelect, useValue } from '../lib/hooks';
import { useProjectionStore } from '../stores';

function MirrorOf(props: { id: string }) {
  const { transport } = useApp();
  const store = useProjectionStore(transport, `pane_text:${props.id}`);
  const state = useValue(store);
  // The key of the last row that has text. The keys are the numbers of the rows.
  let last: string | undefined;
  for (let at = state.order.length - 1; at >= 0; at--) {
    const key = state.order[at];
    if (key !== undefined && (state.byKey[key] ?? '') !== '') {
      last = key;
      break;
    }
  }
  return (
    <div className="mirror" role="log" aria-label="Terminal text">
      {state.order.map((key) => (
        <div key={key} data-row={key} aria-live={key === last ? 'polite' : 'off'}>
          {state.byKey[key]}
        </div>
      ))}
    </div>
  );
}

export function Mirror() {
  const { stores, windowId } = useApp();
  const focused = useSelect(
    stores.windows,
    (state) => state.byKey[windowId]?.focused_pane ?? undefined,
  );
  return focused ? <MirrorOf key={focused} id={focused} /> : null;
}
