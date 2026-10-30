// The keys of a list over the window, in its last row: the arrows move, Enter does the action of
// the row and Escape closes the list. The palette shows it.

export function PaletteKeys(props: { action: string }) {
  return (
    <div className="palette-keys" aria-hidden="true">
      <span>
        <kbd className="cap">↑</kbd>
        <kbd className="cap">↓</kbd> Move
      </span>
      <span>
        <kbd className="cap">↵</kbd> {props.action}
      </span>
      <span>
        <kbd className="cap">esc</kbd> Close
      </span>
    </div>
  );
}
