// The node picker: a menu at a point of the canvas with the items of `workflow.presets`, a filter
// field and keys (type to filter, ↑ ↓, Enter, Esc). A pick adds the node there.

import type { WorkflowNodePreset } from '@neohar/transport';
import { createMemo, createSignal, For, onMount, Show } from 'solid-js';
import { matches, moveIndex } from '../git/model';
import { harnessName, kindName } from './model';

export function NodePicker(props: {
  at: { x: number; y: number };
  presets: readonly WorkflowNodePreset[];
  onPick(preset: WorkflowNodePreset): void;
  onClose(): void;
}) {
  let field: HTMLInputElement | undefined;
  const [filter, setFilter] = createSignal('');
  const [active, setActive] = createSignal(0);
  const shown = createMemo(() =>
    props.presets.filter((preset) =>
      matches(filter(), [
        preset.label,
        kindName(preset.node.kind),
        harnessName(preset.node.harness),
      ]),
    ),
  );
  onMount(() => field?.focus());

  const onKey = (event: KeyboardEvent): void => {
    event.stopPropagation();
    if (event.key === 'Escape') {
      event.preventDefault();
      props.onClose();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      const preset = shown()[active()];
      if (preset) props.onPick(preset);
      return;
    }
    const next = moveIndex(shown().length, active(), event.key);
    if (next !== null) {
      event.preventDefault();
      setActive(next);
    }
  };

  // The menu stays in the window.
  const left = () => Math.max(8, Math.min(props.at.x, window.innerWidth - 280));
  const top = () => Math.max(8, Math.min(props.at.y, window.innerHeight - 320));

  return (
    <div
      class="wf-picker glass"
      role="dialog"
      aria-label="Add a node"
      style={{ left: `${left()}px`, top: `${top()}px` }}
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={onKey}
      onFocusOut={(event) => {
        const next = event.relatedTarget as Node | null;
        if (!next || !event.currentTarget.contains(next)) props.onClose();
      }}
    >
      <input
        class="wf-picker-filter"
        type="text"
        placeholder="Add a node"
        autocomplete="off"
        autocapitalize="off"
        spellcheck={false}
        aria-activedescendant={`wf-pick-${active()}`}
        value={filter()}
        ref={(element) => (field = element)}
        onInput={(event) => {
          setFilter(event.currentTarget.value);
          setActive(0);
        }}
      />
      <ul class="wf-picker-list" role="listbox">
        <For each={shown()}>
          {(preset, index) => (
            <li
              id={`wf-pick-${index()}`}
              role="option"
              class="wf-picker-item"
              classList={{ active: index() === active() }}
              aria-selected={index() === active()}
              onPointerEnter={() => setActive(index())}
              onClick={() => props.onPick(preset)}
            >
              <span class="wf-picker-label">{preset.label}</span>
              <span class="wf-picker-kind">
                {preset.node.kind === 'agent'
                  ? harnessName(preset.node.harness)
                  : kindName(preset.node.kind)}
              </span>
            </li>
          )}
        </For>
        <Show when={shown().length === 0}>
          <li class="wf-picker-empty">No node matches.</li>
        </Show>
      </ul>
    </div>
  );
}
