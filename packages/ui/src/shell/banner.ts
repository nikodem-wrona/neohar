// The look of a banner over the tab bar: the Alert of shadcn/ui as a row of the main panel, with no
// box and a rule under it. The colour says what the banner is: red for a fault, amber for a
// warning that waits for the person.

/** The row: no box, the text of the chrome, and a rule under it. */
export const BANNER =
  'flex items-center gap-[1ch] rounded-none border-0 border-b border-b-(--line) px-2 py-0 text-[length:inherit] leading-5';

/** A fault: a red edge at the left, on the raised panel. */
export const BANNER_FAULT = 'bg-(--bg-raised) text-foreground shadow-[inset_2px_0_0_var(--danger)]';

/** A warning: an amber edge at the left, on the raised panel. */
export const BANNER_WARNING =
  'bg-(--bg-raised) text-foreground shadow-[inset_2px_0_0_var(--caution)]';

/** The hint or the code after the text. */
export const BANNER_HINT = 'banner-hint text-muted-foreground';
