// The field that edits the name of a workspace or a tab. It opens with the old name
// selected. Enter or a click outside the field sends the name, and Escape keeps the old one. When
// the core refuses the name after Enter, the field stays open in red, with the message of the core
// as its title. After a click outside, the field closes and the old name stays: the person went on
// to something else, and an open field would keep the focus from the pane.

import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { Input } from '../components/ui/input';
import { cn } from '../lib/utils';
import { NAME_MAX, requestRename, sameTarget, type RenameTarget } from './rename';

export function RenameField(props: {
  target: RenameTarget;
  /** The name now. */
  name: string;
  /** What the field edits, for a screen reader: "Name of tab w2:t4". */
  label: string;
  className?: string;
  style?: CSSProperties;
}) {
  const { transport, view, report, form } = useApp();
  // A field edits one name: a later target is a new field.
  const [target] = useState(props.target);
  const [failure, setFailure] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  /** A call is on its way, or the field is done: a blur then does nothing. */
  const busy = useRef(false);
  const done = useRef(false);
  /** The focus left the field. */
  const away = useRef(false);
  const finish = (): void => {
    done.current = true;
    if (sameTarget(view.renaming.get(), target)) view.renaming.set(null);
  };
  const commit = (): void => {
    if (busy.current || done.current || !input.current) return;
    busy.current = true;
    requestRename(transport, target, input.current.value, props.name)
      .then((message) => {
        busy.current = false;
        if (message === null || away.current) finish();
        else setFailure(message);
      })
      .catch(report);
  };
  // The old name once, before the paint: a change of the name from another client does not reset
  // the typing.
  const first = useRef(props.name);
  useLayoutEffect(() => {
    const element = input.current;
    if (!element) return;
    element.value = first.current;
    element.focus({ preventScroll: true });
    element.select();
  }, []);
  // A field that goes with its row (the tab closed) leaves the view with no edit.
  useEffect(
    () => () => {
      done.current = true;
      if (sameTarget(view.renaming.get(), target)) view.renaming.set(null);
    },
    [view, target],
  );
  return (
    // The Input of shadcn/ui, with the height of a row. A name that the core refused gives it a
    // red border, and the message is its title. A phone keeps 16 px, so that Safari does not zoom.
    <Input
      ref={input}
      className={cn(
        'rename-field h-[calc(var(--row)-4px)] rounded-(--r-sm) border-foreground bg-background px-(--s1) dark:bg-background py-0 caret-foreground shadow-none',
        form === 'phone' && 'md:text-base',
        props.className,
      )}
      style={props.style}
      type="text"
      maxLength={NAME_MAX}
      aria-label={props.label}
      aria-invalid={failure !== null}
      title={failure ?? undefined}
      autoComplete="off"
      autoCapitalize="off"
      spellCheck={false}
      data-rename={target.id}
      onChange={() => setFailure(null)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          commit();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          finish();
        }
      }}
      onFocus={() => (away.current = false)}
      onBlur={() => {
        away.current = true;
        commit();
      }}
    />
  );
}
