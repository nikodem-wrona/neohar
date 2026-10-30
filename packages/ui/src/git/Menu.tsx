// A menu of the top bar of the Git view: a button that shows the value, and a popover with a filter
// field and a list. A letter filters the items, the arrows move, Enter picks and Escape closes. The
// core gives the items in their order, and the menu only keeps the ones that match the filter. The
// popover is in the top layer, so a click beside it closes it.

import { type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { cn } from '../lib/utils';
import { matches, moveIndex } from './model';

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

export function GitMenu(props: GitMenuProps) {
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState('');
  const [active, setActive] = useState(0);
  const id = `git-menu-${props.label.toLowerCase()}`;

  const shown = filtered(props.groups, filter);
  const flat = shown.flatMap((group) => group.items);
  const actions = props.actions?.(filter) ?? [];

  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const toggle = (event: Event): void => {
      setOpen((event as ToggleEvent).newState === 'open');
    };
    element.addEventListener('toggle', toggle);
    return () => element.removeEventListener('toggle', toggle);
  }, []);

  const close = (): void => {
    if (panel.current?.matches(':popover-open')) panel.current.hidePopover();
  };
  const show = (): void => {
    if (!panel.current || !trigger.current) return;
    const box = trigger.current.getBoundingClientRect();
    panel.current.style.left = `${Math.max(4, Math.min(box.left, window.innerWidth - 360))}px`;
    panel.current.style.top = `${box.bottom + 4}px`;
    setFilter('');
    // The filter is empty from now on: the current item is found in every item.
    const current = filtered(props.groups, '')
      .flatMap((group) => group.items)
      .findIndex((item) => item.current);
    setActive(Math.max(0, current));
    panel.current.showPopover();
    field.current?.focus();
  };
  const pick = (item: MenuItem | undefined): void => {
    if (!item || item.disabled) return;
    close();
    props.onPick(item.key);
    trigger.current?.focus();
  };

  const onKey = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close();
      trigger.current?.focus();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      const item = flat[active];
      if (item) pick(item);
      else actions[0]?.run();
      if (!item) close();
      return;
    }
    const to = moveIndex(flat.length, active, event.key);
    if (to === null) return;
    event.preventDefault();
    setActive(to);
    panel.current
      ?.querySelector<HTMLElement>(`[data-index="${to}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  };

  return (
    <div className="git-menu-anchor">
      <button
        type="button"
        className={cn('git-menu-button', { open })}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={id}
        ref={trigger}
        onClick={() => (open ? close() : show())}
      >
        <span className="git-menu-label">{props.label}</span>
        <span className="git-menu-line">
          <span className="git-menu-value">{props.value}</span>
          {props.note ? <span className="git-menu-note">{props.note}</span> : null}
          <span className="git-menu-arrow" aria-hidden="true">
            ▾
          </span>
        </span>
      </button>
      <div id={id} className="git-menu" popover="auto" ref={panel} onKeyDown={onKey}>
        <input
          className="git-menu-filter"
          type="text"
          placeholder="Filter"
          aria-label={`Filter the ${props.label.toLowerCase()} menu`}
          aria-activedescendant={flat[active] ? `${id}-${active}` : undefined}
          value={filter}
          ref={field}
          onChange={(event) => {
            setFilter(event.currentTarget.value);
            setActive(0);
          }}
        />
        <div className="git-menu-list" role="listbox" aria-label={props.label}>
          {flat.length > 0 ? (
            shown.map((group, at) => (
              <div
                key={group.title ?? at}
                className="git-menu-group"
                role="group"
                aria-label={group.title}
              >
                {group.title ? (
                  <div className="git-menu-group-title" aria-hidden="true">
                    {group.title}
                  </div>
                ) : null}
                {group.items.map((item) => {
                  const index = flat.indexOf(item);
                  return (
                    <div
                      key={item.key}
                      id={`${id}-${index}`}
                      className={cn('git-menu-item', {
                        active: index === active,
                        current: item.current === true,
                        disabled: item.disabled === true,
                      })}
                      role="option"
                      aria-selected={item.current === true}
                      aria-disabled={item.disabled === true}
                      data-index={index}
                      data-key={item.key}
                      onPointerMove={() => setActive(index)}
                      onClick={() => pick(item)}
                    >
                      <span className="git-menu-item-title">{item.title}</span>
                      {item.detail ? (
                        <span className="git-menu-item-detail">{item.detail}</span>
                      ) : null}
                      {item.agents ? (
                        <span className="git-menu-item-agents">{item.agents}</span>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            ))
          ) : (
            <p className="git-menu-empty">{props.empty}</p>
          )}
        </div>
        {actions.length > 0 ? (
          <div className="git-menu-actions">
            {actions.map((action) => (
              <button
                key={action.label}
                type="button"
                className="git-menu-action"
                onClick={() => {
                  close();
                  action.run();
                }}
              >
                {action.label}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
