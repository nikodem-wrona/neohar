// A strip of the board: one agent run, as a controller keeps a paper strip for each
// aircraft in a rack. Its left edge has the colour of its state. The first line has the badge, the
// state in four letters and the work. The second line has the harness and the cost. A strip of one
// line (a narrow board, the list behind `+N`) has the first line, and the cost when it has room.
// A click shows the pane of the run, in any tab. The board shows no handles. The name of
// the button comes from its text, so it holds what the person sees (WCAG 2.5.3): the state in words
// from the badge, the state in four letters and the work.

import type { AgentValue } from '@neohar/transport';
import { memo, useLayoutEffect, useRef, useState } from 'react';
import { AgentBadge } from '../agents/AgentBadge';
import { badgeOf } from '../agents/badge';
import { cn } from '../lib/utils';
import { costText } from '../timeline/model';

export interface StripProps {
  run: AgentValue;
  /** What the run works on: the title of its pane, else the harness. */
  work: string;
  /** The tooltip: the state in words and the usage. */
  tooltip: string;
  /** The run is the agent of the pane that has the focus in this window. */
  focused: boolean;
  /** `two`: the full strip. `one`: the first line and the cost. `narrow`: the first line only. */
  form: 'two' | 'one' | 'narrow';
  onShow: (pane: string) => void;
}

export const Strip = memo(function Strip(props: StripProps) {
  const badge = badgeOf(props.run);
  const cost = costText(props.run.usage);
  const element = useRef<HTMLButtonElement>(null);
  /** The edge flashes now. The class stays when another class of the strip changes. */
  const [flash, setFlash] = useState(false);
  const before = useRef(badge.waiting);

  // The edge flashes twice when the run starts to wait, with the attention line. A strip that is
  // drawn while the run already waits does not flash: nothing changed in front of the person.
  useLayoutEffect(() => {
    const waiting = badge.waiting;
    const was = before.current;
    before.current = waiting;
    const button = element.current;
    if (!button || !waiting || was !== false) return;
    button.classList.remove('flash');
    // A new layout starts the animation again for a run that waits a second time.
    void button.offsetWidth;
    button.classList.add('flash');
    setFlash(true);
  }, [badge.waiting]);

  return (
    <button
      type="button"
      className={cn('strip', { focused: props.focused, waiting: badge.waiting, flash })}
      data-code={badge.code}
      data-agent={props.run.id}
      data-agent-pane={props.run.pane ?? undefined}
      title={props.tooltip}
      disabled={!props.run.pane}
      ref={element}
      onAnimationEnd={(event) => {
        event.currentTarget.classList.remove('flash');
        setFlash(false);
      }}
      onClick={() => {
        if (props.run.pane) props.onShow(props.run.pane);
      }}
    >
      <span className="strip-line">
        <AgentBadge run={props.run} />
        <span className="agent-code" data-code={badge.code}>
          {badge.code}
        </span>
        <span className="strip-work">{props.work}</span>
        {props.form === 'one' && cost ? <span className="strip-cost">{cost}</span> : null}
      </span>
      {props.form === 'two' ? (
        <span className="strip-line strip-meta">
          <span className="strip-harness">{badge.harness}</span>
          {cost ? <span className="strip-cost">{cost}</span> : null}
        </span>
      ) : null}
    </button>
  );
});
