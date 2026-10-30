// The Badge of shadcn/ui (ui.shadcn.com), in Solid, on the tokens of the chrome: a short label in a
// pill. The tone gives its colour, with the meaning that the tokens give each colour. The other
// attributes go to the element.

import { type JSX, splitProps } from 'solid-js';

export type BadgeTone = 'neutral' | 'info' | 'ok' | 'warn' | 'error' | 'muted';

export function Badge(
  props: { tone?: BadgeTone | undefined } & JSX.HTMLAttributes<HTMLSpanElement>,
) {
  const [own, rest] = splitProps(props, ['tone', 'class']);
  return (
    <span
      class={own.class ? `ui-badge ${own.class}` : 'ui-badge'}
      data-tone={own.tone ?? 'neutral'}
      {...rest}
    />
  );
}
