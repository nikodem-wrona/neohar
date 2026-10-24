// A menu of the top bar of the Git view: a button that shows the value, and a popover with a filter
// field and a list. A letter filters the items, the arrows move, Enter picks and Escape closes. The
// core gives the items in their order, and the menu only keeps the ones that match the filter. The
// popover is in the top layer, so a click beside it closes it.

import { createMemo, createSignal, For, Show } from 'solid-js';
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

export function GitMenu(props: GitMenuProps) {
  let trigger: HTMLButtonElement | undefined;
  let panel: HTMLDivElement | undefined;
  let field: HTMLInputElement | undefined;
  const [open, setOpen] = createSignal(false);
  const [filter, setFilter] = createSignal('');
  const [active, setActive] = createSignal(0);
  const id = `git-menu-${props.label.toLowerCase()}`;

  const shown = createMemo(() =>
    props.groups
      .map((group) => ({
        ...group,
        items: group.items.filter((item) =>
          matches(filter(), [item.title, item.detail, item.agents]),
        ),
      }))
      .filter((group) => group.items.length > 0),
  );
  const flat = createMemo(() => shown().flatMap((group) => group.items));
  const actions = () => props.actions?.(filter()) ?? [];

  const close = (): void => {
    if (panel?.matches(':popover-open')) panel.hidePopover();
  };
  const show = (): void => {
    if (!panel || !trigger) return;
    const box = trigger.getBoundingClientRect();
    panel.style.left = `${Math.max(4, Math.min(box.left, window.innerWidth - 360))}px`;
    panel.style.top = `${box.bottom + 4}px`;
    setFilter('');
    const current = flat().findIndex((item) => item.current);
    setActive(Math.max(0, current));
    panel.showPopover();
    field?.focus();
  };
  const pick = (item: MenuItem | undefined): void => {
    if (!item || item.disabled) return;
    close();
    props.onPick(item.key);
    trigger?.focus();
  };

  const onKey = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close();
      trigger?.focus();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      const item = flat()[active()];
      if (item) pick(item);
      else actions()[0]?.run();
      if (!item) close();
      return;
    }
    const to = moveIndex(flat().length, active(), event.key);
    if (to === null) return;
    event.preventDefault();
    setActive(to);
    panel?.querySelector<HTMLElement>(`[data-index="${to}"]`)?.scrollIntoView({ block: 'nearest' });
  };

  return (
    <div class="git-menu-anchor">
      <button
        type="button"
        class="git-menu-button"
        classList={{ open: open() }}
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-controls={id}
        ref={(element) => (trigger = element)}
        onClick={() => (open() ? close() : show())}
      >
        <span class="git-menu-label">{props.label}</span>
        <span class="git-menu-line">
          <span class="git-menu-value">{props.value}</span>
          <Show when={props.note}>{(note) => <span class="git-menu-note">{note()}</span>}</Show>
          <span class="git-menu-arrow" aria-hidden="true">
            ▾
          </span>
        </span>
      </button>
      <div
        id={id}
        class="git-menu"
        popover="auto"
        ref={(element) => {
          panel = element;
          element.addEventListener('toggle', (event) => {
            setOpen((event as ToggleEvent).newState === 'open');
          });
        }}
        onKeyDown={onKey}
      >
        <input
          class="git-menu-filter"
          type="text"
          placeholder="Filter"
          aria-label={`Filter the ${props.label.toLowerCase()} menu`}
          aria-activedescendant={flat()[active()] ? `${id}-${active()}` : undefined}
          value={filter()}
          ref={(element) => (field = element)}
          onInput={(event) => {
            setFilter(event.currentTarget.value);
            setActive(0);
          }}
        />
        <div class="git-menu-list" role="listbox" aria-label={props.label}>
          <Show when={flat().length > 0} fallback={<p class="git-menu-empty">{props.empty}</p>}>
            <For each={shown()}>
              {(group) => (
                <div class="git-menu-group" role="group" aria-label={group.title}>
                  <Show when={group.title}>
                    <div class="git-menu-group-title" aria-hidden="true">
                      {group.title}
                    </div>
                  </Show>
                  <For each={group.items}>
                    {(item) => {
                      const index = () => flat().indexOf(item);
                      return (
                        <div
                          id={`${id}-${index()}`}
                          class="git-menu-item"
                          classList={{
                            active: index() === active(),
                            current: item.current === true,
                            disabled: item.disabled === true,
                          }}
                          role="option"
                          aria-selected={item.current === true}
                          aria-disabled={item.disabled === true}
                          data-index={index()}
                          data-key={item.key}
                          onPointerMove={() => setActive(index())}
                          onClick={() => pick(item)}
                        >
                          <span class="git-menu-item-title">{item.title}</span>
                          <Show when={item.detail}>
                            {(detail) => <span class="git-menu-item-detail">{detail()}</span>}
                          </Show>
                          <Show when={item.agents}>
                            {(agents) => <span class="git-menu-item-agents">{agents()}</span>}
                          </Show>
                        </div>
                      );
                    }}
                  </For>
                </div>
              )}
            </For>
          </Show>
        </div>
        <Show when={actions().length > 0}>
          <div class="git-menu-actions">
            <For each={actions()}>
              {(action) => (
                <button
                  type="button"
                  class="git-menu-action"
                  onClick={() => {
                    close();
                    action.run();
                  }}
                >
                  {action.label}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}
