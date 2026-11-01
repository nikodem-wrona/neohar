// A key cap: the chord of a command, as the Kbd of shadcn/ui. A cap is gray, because a key is not
// live, and the cap of Answer is amber while an agent waits. The chord is in the family of the
// data, as the other codes of the chrome.

import type { ComponentProps } from 'react';
import { Kbd } from '../components/ui/kbd';
import { cn } from '../lib/utils';

export function Cap({
  wait = false,
  className,
  ...rest
}: { wait?: boolean } & ComponentProps<typeof Kbd>) {
  return (
    <Kbd
      className={cn(
        'cap h-4 min-w-[2ch] rounded-[2px] bg-(--cap) px-1 font-(family-name:--font-data) text-(length:--size-data) font-semibold text-foreground',
        wait && 'cap-wait bg-caution text-on-signal',
        className,
      )}
      {...rest}
    />
  );
}
