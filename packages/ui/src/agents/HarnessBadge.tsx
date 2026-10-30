// The badge of a harness: its mark and its name in a Badge of the kit, so the strip of a pane says
// at a glance if Claude Code or Codex runs in it. The mark of Claude Code is a spark, the mark of
// Codex is a prompt, and another harness has a dot. The marks take the colour of the text: a colour
// in the chrome tells a state, not a harness. The tooltip is the one of the badge of the run.

import type { AgentValue } from '@neohar/transport';
import { Match, Switch } from 'solid-js';
import { Badge } from '../kit/Badge';
import { badgeOf, harnessName } from './badge';

export function HarnessBadge(props: { run: AgentValue; class?: string | undefined }) {
  return (
    <Badge
      class={props.class ? `harness-badge ${props.class}` : 'harness-badge'}
      data-harness={props.run.harness}
      title={badgeOf(props.run).tooltip}
    >
      <HarnessMark harness={props.run.harness} />
      {harnessName(props.run.harness)}
    </Badge>
  );
}

export function HarnessMark(props: { harness: string }) {
  return (
    <svg class="harness-mark" viewBox="0 0 16 16" aria-hidden="true">
      <Switch fallback={<circle cx="8" cy="8" r="2.5" />}>
        <Match when={props.harness === 'claude'}>
          <path d="M8 2v12M2 8h12M3.75 3.75l8.5 8.5M12.25 3.75l-8.5 8.5" />
        </Match>
        <Match when={props.harness === 'codex'}>
          <path d="M3 4.5L6.5 8 3 11.5M8.5 12H13" />
        </Match>
      </Switch>
    </svg>
  );
}
