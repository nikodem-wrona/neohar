// The text mirror for screen readers. An element that is off the screen holds the
// text of the visible rows of the focused pane. The core sends the rows at most 4 times a second,
// and only for a pane that someone follows, so the page subscribes for the focused pane only. The
// last row that has text is a live region, so a screen reader speaks new output.

import { createMemo, For, onCleanup, Show } from 'solid-js';
import { useApp } from '../app/context';
import { createProjectionStore } from '../stores';

function MirrorOf(props: { id: string }) {
  const { transport } = useApp();
  const store = createProjectionStore(transport, `pane_text:${props.id}`);
  onCleanup(() => store.close());
  // The key of the last row that has text. The keys are the numbers of the rows.
  const last = createMemo(() => {
    const order = store.state.order;
    for (let at = order.length - 1; at >= 0; at--) {
      const key = order[at];
      if (key !== undefined && (store.state.byKey[key] ?? '') !== '') return key;
    }
    return undefined;
  });
  return (
    <div class="mirror" role="log" aria-label="Terminal text">
      <For each={store.state.order}>
        {(key) => (
          <div data-row={key} aria-live={key === last() ? 'polite' : 'off'}>
            {store.state.byKey[key]}
          </div>
        )}
      </For>
    </div>
  );
}

export function Mirror() {
  const { stores, windowId } = useApp();
  const focused = () => stores.windows.state.byKey[windowId]?.focused_pane ?? undefined;
  return (
    <Show when={focused()} keyed>
      {(id) => <MirrorOf id={id} />}
    </Show>
  );
}
