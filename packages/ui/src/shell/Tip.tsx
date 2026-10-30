// The tip of a button that shows an icon only: the Tooltip of shadcn/ui in place of a `title`. The
// button keeps its `aria-label`, so a screen reader reads the same name. The tip shows for the
// pointer and for the focus of the keyboard, and not for a tap of a finger. `TooltipProvider` is
// in the shell. A button that opens a layer of its own (a popover, a menu) hides its tip while the
// layer is open: else the tip is the top layer, and Escape closes the tip and not the layer.

import { type ReactElement, useState } from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip';

export function Tip(props: {
  text: string;
  side?: 'top' | 'right' | 'bottom' | 'left';
  /** The layer of the button is open: the tip does not show. */
  hidden?: boolean;
  children: ReactElement;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Tooltip open={open && !props.hidden} onOpenChange={setOpen}>
      <TooltipTrigger asChild>{props.children}</TooltipTrigger>
      <TooltipContent side={props.side ?? 'bottom'} sideOffset={4}>
        {props.text}
      </TooltipContent>
    </Tooltip>
  );
}
