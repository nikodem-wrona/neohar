// A modal `<dialog>` of the chrome: the question of a close and the task dialog.

/** A press on the backdrop of a modal dialog: the dialog is the target, outside its box. */
export function outside(event: MouseEvent, dialog: HTMLElement): boolean {
  const box = dialog.getBoundingClientRect();
  return (
    event.clientX < box.left ||
    event.clientX > box.right ||
    event.clientY < box.top ||
    event.clientY > box.bottom
  );
}
