// The Badge of shadcn/ui (ui.shadcn.com), in React, on the tokens of the chrome: a short label in a
// pill. The tone gives its colour, with the meaning that the tokens give each colour. The other
// attributes go to the element.

import type { ComponentProps } from 'react';

export type BadgeTone = 'neutral' | 'info' | 'ok' | 'warn' | 'error' | 'muted';

export function Badge({
  tone,
  className,
  ...rest
}: { tone?: BadgeTone | undefined } & ComponentProps<'span'>) {
  return (
    <span
      className={className ? `ui-badge ${className}` : 'ui-badge'}
      data-tone={tone ?? 'neutral'}
      {...rest}
    />
  );
}
