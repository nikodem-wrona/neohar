// The field that edits the name of a workspace or a tab. It opens with the old name
// selected. Enter or a click outside the field sends the name, and Escape keeps the old one. When
// the core refuses the name after Enter, the field stays open in red, with the message of the core
// as its title. After a click outside, the field closes and the old name stays: the person went on
// to something else, and an open field would keep the focus from the pane.

import { createSignal, onCleanup, onMount, type JSX } from 'solid-js';
import { useApp } from '../app/context';
import { NAME_MAX, requestRename, sameTarget, type RenameTarget } from './rename';

export function RenameField(props: {
  target: RenameTarget;
  /** The name now. */
  name: string;
  /** What the field edits, for a screen reader: "Name of tab w2:t4". */
  label: string;
  class?: string;
  style?: JSX.CSSProperties;
}) {
  const { transport, view, report } = useApp();
  // A field edits one name: a later target is a new field.
  const target = props.target;
  const [failure, setFailure] = createSignal<string | null>(null);
  let input: HTMLInputElement | undefined;
  /** A call is on its way, or the field is done: a blur then does nothing. */
  let busy = false;
  let done = false;
  /** The focus left the field. */
  let away = false;
  const finish = (): void => {
    done = true;
    if (sameTarget(view.renaming(), target)) view.setRenaming(null);
  };
  const commit = (): void => {
    if (busy || done || !input) return;
    busy = true;
    requestRename(transport, target, input.value, props.name)
      .then((message) => {
        busy = false;
        if (message === null || away) finish();
        else setFailure(message);
      })
      .catch(report);
  };
  onMount(() => {
    if (!input) return;
    // The old name once: a change of the name from another client does not reset the typing.
    input.value = props.name;
    input.focus({ preventScroll: true });
    input.select();
  });
  // A field that goes with its row (the tab closed) leaves the view with no edit.
  onCleanup(finish);
  return (
    <input
      ref={(element) => (input = element)}
      class={`rename-field ${props.class ?? ''}`}
      style={props.style}
      type="text"
      maxLength={NAME_MAX}
      aria-label={props.label}
      aria-invalid={failure() !== null}
      title={failure() ?? undefined}
      autocomplete="off"
      autocapitalize="off"
      spellcheck={false}
      data-rename={target.id}
      onInput={() => setFailure(null)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          commit();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          finish();
        }
      }}
      onFocus={() => (away = false)}
      onBlur={() => {
        away = true;
        commit();
      }}
    />
  );
}
