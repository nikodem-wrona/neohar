// The Button of shadcn/ui (ui.shadcn.com), in Solid, on the tokens of the chrome. `default` is the
// one action of a card or a dialog, in reverse white; `outline` and `ghost` are the others. `icon`
// and `icon-sm` are square, for a button with only an icon. The other attributes go to the element,
// and the type is `button` unless they say otherwise.

import { type JSX, splitProps } from 'solid-js';

export type ButtonVariant = 'default' | 'outline' | 'ghost';
export type ButtonSize = 'default' | 'sm' | 'icon' | 'icon-sm';

export function Button(
  props: {
    variant?: ButtonVariant | undefined;
    size?: ButtonSize | undefined;
  } & JSX.ButtonHTMLAttributes<HTMLButtonElement>,
) {
  const [own, rest] = splitProps(props, ['variant', 'size', 'class', 'type']);
  return (
    <button
      type={own.type ?? 'button'}
      class={own.class ? `ui-button ${own.class}` : 'ui-button'}
      data-variant={own.variant ?? 'default'}
      data-size={own.size ?? 'default'}
      {...rest}
    />
  );
}
