// The Input of shadcn/ui (ui.shadcn.com), in Solid, on the tokens of the chrome: a field of one
// line with a ring for the focus. The attributes go to the element.

import { type JSX, splitProps } from 'solid-js';

export function Input(props: JSX.InputHTMLAttributes<HTMLInputElement>) {
  const [own, rest] = splitProps(props, ['class', 'type']);
  return (
    <input
      type={own.type ?? 'text'}
      class={own.class ? `ui-input ${own.class}` : 'ui-input'}
      {...rest}
    />
  );
}
