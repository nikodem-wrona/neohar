// The rule between a side column and the panes. A drag changes the width of the column in whole
// characters, and the arrow keys change it by one character (five with Shift). The width is a
// state of this view only (`app/view.ts`): the core does not know the chrome.

import { COLUMN_LIMITS, type Columns } from '../app/view';
import { useApp } from '../app/context';

/** The width of one character of the chrome font, in CSS pixels. */
function charWidth(within: Element): number {
  const probe = document.createElement('span');
  probe.className = 'char-probe';
  probe.textContent = '0'.repeat(20);
  within.append(probe);
  const width = probe.getBoundingClientRect().width / 20;
  probe.remove();
  return width > 0 ? width : 7;
}

export function Gutter(props: {
  column: keyof Columns;
  /** `1` when a drag to the right makes the column wider, `-1` when it makes it narrower. */
  direction: 1 | -1;
  label: string;
}) {
  const { view } = useApp();
  const width = () => view.columns()[props.column];

  const onPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0) return;
    const rule = event.currentTarget as HTMLElement;
    rule.setPointerCapture(event.pointerId);
    rule.classList.add('dragging');
    const step = charWidth(rule.parentElement ?? document.body);
    const startX = event.clientX;
    const startWidth = width();
    const move = (moved: PointerEvent): void =>
      view.setColumn(
        props.column,
        startWidth + (props.direction * (moved.clientX - startX)) / step,
      );
    const end = (): void => {
      rule.classList.remove('dragging');
      rule.removeEventListener('pointermove', move);
      rule.removeEventListener('pointerup', end);
      rule.removeEventListener('pointercancel', end);
    };
    rule.addEventListener('pointermove', move);
    rule.addEventListener('pointerup', end);
    rule.addEventListener('pointercancel', end);
  };

  const onKeyDown = (event: KeyboardEvent): void => {
    const by = event.shiftKey ? 5 : 1;
    if (event.key === 'ArrowLeft') view.setColumn(props.column, width() - props.direction * by);
    else if (event.key === 'ArrowRight')
      view.setColumn(props.column, width() + props.direction * by);
    else return;
    event.preventDefault();
  };

  return (
    <div
      class="gutter"
      data-column={props.column}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      aria-valuenow={width()}
      aria-valuemin={COLUMN_LIMITS[props.column].min}
      aria-valuemax={COLUMN_LIMITS[props.column].max}
      tabindex="0"
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    />
  );
}
