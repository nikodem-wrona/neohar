// The badge of an agent: the glyph of its state with its marks. The board and the
// title strip of a pane use it. The mark of a version that is not validated is in the strip of the
// pane only, because it is a fact about the harness and not a state of the run.

import type { AgentValue } from '@neohar/transport';
import { cn } from '../lib/utils';
import { badgeOf } from './badge';

export function AgentBadge(props: { run: AgentValue }) {
  const badge = badgeOf(props.run);
  return (
    <span
      className={cn('agent-badge', {
        'agent-inferred': badge.inferred,
        'agent-done': badge.done,
        'agent-exited': badge.exited,
        'agent-suggested': badge.suggested,
      })}
      data-state={props.run.facets.execution.value.state}
      role="img"
      aria-label={badge.tooltip}
      title={badge.tooltip}
    >
      {badge.inferred ? (
        <span className="agent-mark" aria-hidden="true">
          ~
        </span>
      ) : null}
      <span className="agent-glyph" aria-hidden="true">
        {badge.glyph}
      </span>
      {badge.done ? <span className="agent-dot" aria-hidden="true" /> : null}
    </span>
  );
}
