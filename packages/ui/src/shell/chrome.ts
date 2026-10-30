// The look of a Button of shadcn/ui in the chrome: no fill and dim text, the text of the chrome,
// and its icon at the size of its SVG. A control adds its place and its states.

export const CHROME_BUTTON =
  "h-auto gap-0 rounded-(--r-sm) p-0 text-[length:inherit] font-normal text-muted-foreground hover:bg-transparent hover:text-foreground has-[>svg]:p-0 dark:hover:bg-transparent [&_svg:not([class*='size-'])]:size-auto";

/** A button of a rail: a square of a row, with 6 px around the icon. */
export const RAIL_BUTTON =
  'rail-button flex-col gap-0.5 rounded-(--r-tab) py-1.5 has-[>svg]:py-1.5 hover:bg-(--bg-hover) dark:hover:bg-(--bg-hover)';
