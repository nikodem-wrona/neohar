// A menu of the top bar of the Git view: a button that shows the value, and a Popover with a
// Command of shadcn/ui: a filter field and a list. A letter filters the items, the arrows move,
// Enter picks and Escape closes. The core gives the items in their order, and the menu only keeps
// the ones that match the filter (`matches` of the model): the Command does not filter or sort.
// The popover is in a layer over the window, so a click beside it closes it.

import { useState } from 'react';
import { useLayer } from '../app/layer';
import { Button } from '../components/ui/button';
import {
  Command,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '../components/ui/command';
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover';
import { cn } from '../lib/utils';
import { matches } from './model';

export interface MenuItem {
  key: string;
  title: string;
  /** The facts of the item, after its title. */
  detail?: string | undefined;
  /** The agents of the item, on a line of their own. */
  agents?: string | null | undefined;
  current?: boolean | undefined;
  disabled?: boolean | undefined;
}

export interface MenuGroup {
  title?: string;
  items: MenuItem[];
}

export interface MenuAction {
  label: string;
  run: () => void;
}

export interface GitMenuProps {
  /** The name of the menu, for example "Branch". */
  label: string;
  value: string;
  /** A note after the value, for example the agents or "1↑ 3↓". */
  note?: string | null | undefined;
  groups: MenuGroup[];
  onPick: (key: string) => void;
  /** The actions after the list. `filter` is the text in the field. */
  actions?: (filter: string) => MenuAction[];
  /** The text of a menu with no item. */
  empty: string;
}

/** The groups with the items that match `filter`, and no empty group. */
function filtered(groups: MenuGroup[], filter: string): MenuGroup[] {
  return groups
    .map((group) => ({
      ...group,
      items: group.items.filter((item) => matches(filter, [item.title, item.detail, item.agents])),
    }))
    .filter((group) => group.items.length > 0);
}

/** The value of an action in the Command: it comes after the items. */
function actionValue(index: number): string {
  return `\u0000action ${index}`;
}

export function GitMenu(props: GitMenuProps) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState('');
  /** The value of the item under the keys: its key, or the value of an action. */
  const [active, setActive] = useState('');
  useLayer(open);

  const shown = filtered(props.groups, filter);
  const flat = shown.flatMap((group) => group.items);
  const actions = props.actions?.(filter) ?? [];

  /** The first item of a filter, else its first action, has the keys. */
  const firstOf = (text: string): string => {
    const first = filtered(props.groups, text)[0]?.items[0];
    if (first) return first.key;
    return (props.actions?.(text) ?? []).length > 0 ? actionValue(0) : '';
  };

  const onOpenChange = (next: boolean): void => {
    if (next) {
      // The filter is empty when the menu opens, and the current item has the keys.
      setFilter('');
      const current = filtered(props.groups, '')
        .flatMap((group) => group.items)
        .find((item) => item.current);
      setActive(current?.key ?? firstOf(''));
    }
    setOpen(next);
  };
  const pick = (item: MenuItem): void => {
    if (item.disabled) return;
    // The popover gives the focus back to its button when it closes.
    setOpen(false);
    props.onPick(item.key);
  };

  return (
    <div className="min-w-0">
      <Popover open={open} onOpenChange={onOpenChange}>
        <PopoverTrigger asChild>
          {/* The name of the menu over its value, as the menus of GitHub Desktop. */}
          <Button
            variant="ghost"
            className="h-auto max-w-[40ch] min-w-0 shrink flex-col items-start gap-0 px-2 py-0.5 text-left text-sm/4 font-normal data-[state=open]:bg-accent"
          >
            <span className="text-(length:--size-small) text-muted-foreground">{props.label}</span>
            <span className="flex max-w-full min-w-0 items-baseline gap-[1ch]">
              <span className="min-w-0 truncate font-semibold">{props.value}</span>
              {props.note ? (
                <span className="flex-none text-(length:--size-small) whitespace-nowrap text-muted-foreground">
                  {props.note}
                </span>
              ) : null}
              <span className="flex-none text-muted-foreground" aria-hidden="true">
                ▾
              </span>
            </span>
          </Button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          aria-label={props.label}
          className="flex max-h-[min(60vh,520px)] w-[min(52ch,90vw)] flex-col rounded-(--r-md) p-1"
        >
          <Command
            shouldFilter={false}
            value={active}
            onValueChange={setActive}
            className="min-h-0"
          >
            <CommandInput
              placeholder="Filter"
              aria-label={`Filter the ${props.label.toLowerCase()} menu`}
              value={filter}
              onValueChange={(text) => {
                setFilter(text);
                setActive(firstOf(text));
              }}
            />
            <CommandList aria-label={props.label} className="max-h-none min-h-0 flex-1 py-1">
              {flat.length > 0 ? (
                shown.map((group, at) => (
                  <CommandGroup
                    key={group.title ?? at}
                    heading={group.title}
                    className="p-0 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:pt-1 [&_[cmdk-group-heading]]:pb-0"
                  >
                    {group.items.map((item) => (
                      <CommandItem
                        key={item.key}
                        value={item.key}
                        disabled={item.disabled === true}
                        data-key={item.key}
                        onSelect={() => pick(item)}
                        className="grid cursor-pointer grid-cols-[minmax(0,max-content)_minmax(0,1fr)] gap-x-[1ch] gap-y-0 px-2 py-0.5 text-sm data-[disabled=true]:cursor-default data-[disabled=true]:text-muted-foreground data-[disabled=true]:opacity-100"
                      >
                        <span
                          className={cn(
                            'max-w-[36ch] truncate',
                            item.current === true && "font-semibold before:content-['✓_']",
                          )}
                        >
                          {item.title}
                        </span>
                        {item.detail ? (
                          <span className="min-w-0 truncate text-right font-mono text-(length:--size-data) text-muted-foreground">
                            {item.detail}
                          </span>
                        ) : null}
                        {item.agents ? (
                          <span className="col-span-full text-(length:--size-small) text-muted-foreground">
                            {item.agents}
                          </span>
                        ) : null}
                      </CommandItem>
                    ))}
                  </CommandGroup>
                ))
              ) : (
                <p className="px-2 py-1 text-muted-foreground">{props.empty}</p>
              )}
            </CommandList>
            {actions.length > 0 ? (
              <>
                <CommandSeparator className="mx-0" />
                <CommandGroup className="p-0 pt-1">
                  {actions.map((action, index) => (
                    <CommandItem
                      key={action.label}
                      value={actionValue(index)}
                      className="cursor-pointer px-2 py-0.5 text-sm data-[selected=true]:bg-primary data-[selected=true]:text-primary-foreground"
                      onSelect={() => {
                        setOpen(false);
                        action.run();
                      }}
                    >
                      {action.label}
                    </CommandItem>
                  ))}
                </CommandGroup>
              </>
            ) : null}
          </Command>
        </PopoverContent>
      </Popover>
    </div>
  );
}
