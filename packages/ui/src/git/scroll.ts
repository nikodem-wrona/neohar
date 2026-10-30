// The class of a list of the Git view in a Scroll Area of shadcn/ui.

/**
 * A list that takes the rest of its side. Radix puts the rows in a box with `display: table`,
 * which grows to the longest row: `block` keeps the width of the side, so a long name ends in an
 * ellipsis and the side does not scroll to the side.
 */
export const SCROLL_LIST = 'min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]>div]:block!';
