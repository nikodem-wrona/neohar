// The font of the terminal when the settings name none. A module of its own, so that
// the shell reads it without the renderer: the phone loads the renderer after its first paint.

/** The monospace family of the terminal, with fallbacks. */
export const DEFAULT_FONT_FAMILY =
  '"Hack Nerd Font Mono", Menlo, "SF Mono", "DejaVu Sans Mono", "Symbols Nerd Font Mono", monospace';

/** The size of the font in CSS pixels. */
export const DEFAULT_FONT_PX = 13;
