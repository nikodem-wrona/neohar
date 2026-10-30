// The Button of shadcn/ui (ui.shadcn.com), in React, on the tokens of the chrome. `default` is the
// one action of a card or a dialog, in reverse white; `outline` and `ghost` are the others. `icon`
// and `icon-sm` are square, for a button with only an icon. The other attributes go to the element,
// and the type is `button` unless they say otherwise.

import type { ComponentProps } from 'react';

export type ButtonVariant = 'default' | 'outline' | 'ghost';
export type ButtonSize = 'default' | 'sm' | 'icon' | 'icon-sm';

export function Button({
  variant,
  size,
  className,
  type,
  ...rest
}: {
  variant?: ButtonVariant | undefined;
  size?: ButtonSize | undefined;
} & ComponentProps<'button'>) {
  return (
    <button
      type={type ?? 'button'}
      className={className ? `ui-button ${className}` : 'ui-button'}
      data-variant={variant ?? 'default'}
      data-size={size ?? 'default'}
      {...rest}
    />
  );
}
