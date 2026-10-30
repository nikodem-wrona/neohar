// The Input of shadcn/ui (ui.shadcn.com), in React, on the tokens of the chrome: a field of one
// line with a ring for the focus. The attributes go to the element.

import type { ComponentProps } from 'react';

export function Input({ className, type, ...rest }: ComponentProps<'input'>) {
  return (
    <input
      type={type ?? 'text'}
      className={className ? `ui-input ${className}` : 'ui-input'}
      {...rest}
    />
  );
}
