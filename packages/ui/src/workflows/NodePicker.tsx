// The node picker: a menu at a point of the canvas with the items of `workflow.presets`, a filter
// field and keys (type to filter, ↑ ↓, Enter, Esc). A pick adds the node there.
//
// It is a Popover with a Command of shadcn/ui at the point. The Command does not filter: the
// filter of the field is the pure match of `git/model.ts`, and the items keep the order of the
// core.

import type { WorkflowNodePreset } from '@neohar/transport';
import { useMemo, useState } from 'react';
import { useLayer } from '../app/layer';
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from '../components/ui/command';
import { Popover, PopoverAnchor, PopoverContent } from '../components/ui/popover';
import { matches } from '../git/model';
import { harnessName, kindName } from './model';

export function NodePicker(props: {
  at: { x: number; y: number };
  presets: readonly WorkflowNodePreset[];
  onPick(preset: WorkflowNodePreset): void;
  onClose(): void;
}) {
  const [filter, setFilter] = useState('');
  const [active, setActive] = useState('');
  const shown = props.presets.filter((preset) =>
    matches(filter, [preset.label, kindName(preset.node.kind), harnessName(preset.node.harness)]),
  );
  // The item that Enter picks: the one that the keys or the pointer chose, else the first one.
  const selected = shown.some((preset) => preset.id === active) ? active : (shown[0]?.id ?? '');
  // The element that had the focus before the picker, usually the canvas. It gets the focus back.
  const [before] = useState(() => document.activeElement);
  useLayer(true);

  // The menu opens at the point. Radix keeps it in the window.
  const { x, y } = props.at;
  const anchor = useMemo(
    () => ({ current: { getBoundingClientRect: () => DOMRect.fromRect({ x, y }) } }),
    [x, y],
  );

  return (
    <Popover
      open
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
    >
      <PopoverAnchor virtualRef={anchor} />
      <PopoverContent
        side="bottom"
        align="start"
        sideOffset={0}
        collisionPadding={8}
        className="w-[260px] rounded-md p-1"
        aria-label="Add a node"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (before instanceof HTMLElement) before.focus();
        }}
        // The pointer and the keys of the picker do not reach the canvas under it.
        onPointerDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <Command shouldFilter={false} value={selected} onValueChange={setActive}>
          <CommandInput
            placeholder="Add a node"
            autoCapitalize="off"
            value={filter}
            onValueChange={(value) => {
              setFilter(value);
              setActive('');
            }}
          />
          <CommandList className="mt-1 max-h-[280px]">
            {shown.map((preset) => (
              <CommandItem
                key={preset.id}
                value={preset.id}
                className="justify-between gap-2 px-2 py-1"
                onSelect={() => props.onPick(preset)}
              >
                <span>{preset.label}</span>
                <span className="text-[length:var(--size-small)] text-muted-foreground">
                  {preset.node.kind === 'agent'
                    ? harnessName(preset.node.harness)
                    : kindName(preset.node.kind)}
                </span>
              </CommandItem>
            ))}
            <CommandEmpty className="px-2 py-1 text-[length:var(--size-small)] text-muted-foreground">
              No node matches.
            </CommandEmpty>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
