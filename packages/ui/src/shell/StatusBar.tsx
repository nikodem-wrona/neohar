// The status bar: the function keys on the left, then the limits of the harnesses, the warnings of
// the core, the renderer when it fell back to `canvas2d`, the machine, and, after a rule, the
// switches of the Git view and of the Workflows view as one group. The left column and the drawer
// fold and open from their own arrows and rails. A key cap shows the chord of the settings and runs
// the same command when it is clicked. The caps are gray, and the cap of Answer is amber while an
// agent waits.
// The bar shows what the projections say and decides nothing. Its controls are Buttons of shadcn/ui,
// the facts are Badges, and a Tooltip gives the usage of a limit and the text of the warnings.

import { Fragment } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { Badge } from '../components/ui/badge';
import { Button } from '../components/ui/button';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { hasUsage, limitText, usageText } from '../timeline/model';
import { useProjectionStore } from '../stores';
import { needsYou } from '../workflows/model';
import { Cap } from './Cap';
import { chordText, FUNCTION_KEYS } from './keys';
import { Tip } from './Tip';

/** A switch of a view: dim, and on the card of the chrome while the view shows. */
const VIEW_SWITCH =
  'drawer-toggle h-(--row) rounded-[4px] px-(--s3) text-[length:inherit] font-normal text-muted-foreground hover:bg-transparent hover:text-foreground dark:hover:bg-transparent aria-pressed:bg-(--bg-card) aria-pressed:text-foreground';

export function StatusBar(props: { actions: ActionContext; keymap: Keymap }) {
  const { stores, view, platform, form, transport } = useApp();
  // The switch of the Workflows view has a mark while an execution is paused or waits for you.
  const executions = useProjectionStore(transport, 'workflow_executions');
  const workflowsWait = useRows(executions).some(needsYou);
  // The switch of the Hermes view has a mark while a chat waits for an approval.
  const hermesWait = useRows(useProjectionStore(transport, 'hermes_chats')).some(
    (chat) => chat.state === 'waiting',
  );
  // The host serves `link` to each window. Without a host (the dev WebSocket) the window is local.
  const machine = useSelect(stores.link, (state) => state.byKey['_']?.text ?? 'local');
  const health = useSelect(stores.health, (state) => state.byKey['_']);
  const warnings = health?.warnings ?? [];
  const renderer = health?.renderer;
  const waiting = useSelect(stores.attention, (state) => state.order.length > 0);
  const limits = useRows(stores.limits);
  const passNext = useValue(view.passNext);
  const gitOpen = useValue(view.gitOpen);
  const workflowsOpen = useValue(view.workflowsOpen);
  const hermesOpen = useValue(view.hermesOpen);
  return (
    <footer className="statusbar">
      <div className="fkeys" role="toolbar" aria-label="Function keys">
        {FUNCTION_KEYS.map((key) => {
          const chord = chordText(props.keymap, key.command, platform);
          return chord ? (
            // A key that moves leaves the strip under 1280 px.
            <Button
              key={key.command}
              variant="ghost"
              className={cn(
                'fkey group h-auto gap-[0.5ch] rounded-none p-0 pr-0.5 text-[length:inherit] font-normal text-foreground hover:bg-transparent dark:hover:bg-transparent',
                key.optional === true && 'optional max-[1279px]:hidden',
              )}
              data-command={key.command}
              onClick={() => runCommand(key.command, props.actions)}
            >
              <Cap
                wait={key.waits === true && waiting}
                className="group-hover:outline group-hover:outline-foreground"
              >
                {chord}
              </Cap>
              {key.label}
            </Button>
          ) : null;
        })}
      </div>
      {passNext ? (
        <Badge
          variant="default"
          className="status-pass rounded-(--r-sm) border-0 px-1 py-0 text-[length:inherit] font-normal"
          role="status"
        >
          Next key goes to the pane
        </Badge>
      ) : null}
      <span className="status-spacer" />
      {limits.map((limit) => {
        const text = limitText(limit, harnessName(limit.harness));
        if (!text) return null;
        const badge = (
          <Badge variant="muted" className="status-limit">
            {text}
          </Badge>
        );
        return hasUsage(limit.usage_today) ? (
          <Tip
            key={`${limit.harness}:${limit.account}`}
            text={`Today: ${usageText(limit.usage_today)}`}
            side="top"
          >
            {badge}
          </Tip>
        ) : (
          <Fragment key={`${limit.harness}:${limit.account}`}>{badge}</Fragment>
        );
      })}
      {warnings.length > 0 ? (
        // Red: the warnings are faults of the core. The tip has the text of each.
        <Tip text={warnings.map((w) => w.text).join('\n')} side="top">
          <Badge
            variant="outline"
            className="status-warnings border-destructive/40 text-destructive"
          >
            {warnings.length} {warnings.length === 1 ? 'warning' : 'warnings'}
          </Badge>
        </Tip>
      ) : null}
      {renderer !== 'webgl2' ? (
        <Badge variant="muted" className="status-renderer">
          {renderer ?? 'no renderer'}
        </Badge>
      ) : null}
      <span className="status-machine">{machine}</span>
      {form !== 'phone' ? (
        <div className="status-views" role="group" aria-label="Views">
          <Tip text="The Git view of the folder of this window" side="top">
            <Button
              variant="ghost"
              className={VIEW_SWITCH}
              aria-pressed={gitOpen}
              onClick={() => props.actions.toggleGit()}
            >
              Git
            </Button>
          </Tip>
          <Tip
            text={
              workflowsWait
                ? 'An execution of a workflow is paused or waits for you'
                : 'The workflows of agents, and their executions in the folder of this window'
            }
            side="top"
          >
            <Button
              variant="ghost"
              className={cn(VIEW_SWITCH, { 'wf-waiting': workflowsWait })}
              aria-pressed={workflowsOpen}
              onClick={() => props.actions.toggleWorkflows()}
            >
              Workflows
            </Button>
          </Tip>
          <Tip
            text={
              hermesWait
                ? 'A chat with Hermes waits for an approval'
                : 'The chats with Hermes Agent'
            }
            side="top"
          >
            <Button
              variant="ghost"
              className={cn(VIEW_SWITCH, { 'wf-waiting': hermesWait })}
              aria-pressed={hermesOpen}
              onClick={() => props.actions.toggleHermes()}
            >
              Hermes
            </Button>
          </Tip>
        </div>
      ) : null}
    </footer>
  );
}
