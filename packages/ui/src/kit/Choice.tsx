// The choice card of shadcn/ui (ui.shadcn.com, a Field with a RadioGroupItem or a Checkbox in a
// FieldLabel), in Solid, on the tokens of the chrome: an option with its title and its
// description, and the radio button or the check box at the end. The whole card is the label,
// so a click anywhere on it chooses. The control is a native input, so the keyboard and a screen
// reader treat it as one.

import { Show } from 'solid-js';

export function Choice(props: {
  type: 'radio' | 'checkbox';
  name: string;
  title: string;
  description?: string | null | undefined;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <label class="ui-choice">
      <span class="ui-choice-text">
        <span class="ui-choice-title">{props.title}</span>
        <Show when={props.description}>
          <span class="ui-choice-description">{props.description}</span>
        </Show>
      </span>
      <input
        class="ui-choice-control"
        type={props.type}
        name={props.name}
        checked={props.checked}
        onChange={() => props.onChange()}
      />
    </label>
  );
}
