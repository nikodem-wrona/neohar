// The status bar: the function keys on the left, then the limits of the harnesses, the warnings of
// the core, the renderer when it fell back to `canvas2d`, the machine, and, after a rule, the
// switches of the Git view and of the Workflows view as one group. The left column and the drawer
// fold and open from their own arrows and rails. A key cap shows the chord of the settings and runs
// the same command when it is clicked. The caps are gray, and the cap of Answer is amber while an
// agent waits.
// The bar shows what the projections say and decides nothing.

import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { hasUsage, limitText, usageText } from '../timeline/model';
import { useProjectionStore } from '../stores';
import { needsYou } from '../workflows/model';
import { chordText, FUNCTION_KEYS } from './keys';

export function StatusBar(props: { actions: ActionContext; keymap: Keymap }) {
  const { stores, view, platform, form, transport } = useApp();
  // The switch of the Workflows view has a mark while an execution is paused or waits for you.
  const executions = useProjectionStore(transport, 'workflow_executions');
  const workflowsWait = useRows(executions).some(needsYou);
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
  return (
    <footer className="statusbar">
      <div className="fkeys" role="toolbar" aria-label="Function keys">
        {FUNCTION_KEYS.map((key) => {
          const chord = chordText(props.keymap, key.command, platform);
          return chord ? (
            <button
              key={key.command}
              type="button"
              className={cn('fkey', { optional: key.optional === true })}
              data-command={key.command}
              onClick={() => runCommand(key.command, props.actions)}
            >
              <kbd className={cn('cap', { 'cap-wait': key.waits === true && waiting })}>
                {chord}
              </kbd>
              {key.label}
            </button>
          ) : null;
        })}
      </div>
      {passNext ? (
        <span className="status-pass" role="status">
          Next key goes to the pane
        </span>
      ) : null}
      <span className="status-spacer" />
      {limits.map((limit) => {
        const text = limitText(limit, harnessName(limit.harness));
        return text ? (
          <span
            key={`${limit.harness}:${limit.account}`}
            className="status-limit"
            title={
              hasUsage(limit.usage_today) ? `Today: ${usageText(limit.usage_today)}` : undefined
            }
          >
            {text}
          </span>
        ) : null;
      })}
      {warnings.length > 0 ? (
        <span className="status-warnings" title={warnings.map((w) => w.text).join('\n')}>
          {warnings.length} {warnings.length === 1 ? 'warning' : 'warnings'}
        </span>
      ) : null}
      {renderer !== 'webgl2' ? (
        <span className="status-renderer">{renderer ?? 'no renderer'}</span>
      ) : null}
      <span className="status-machine">{machine}</span>
      {form !== 'phone' ? (
        <div className="status-views" role="group" aria-label="Views">
          <button
            type="button"
            className="drawer-toggle"
            aria-pressed={gitOpen}
            title="The Git view of the folder of this window"
            onClick={() => props.actions.toggleGit()}
          >
            Git
          </button>
          <button
            type="button"
            className={cn('drawer-toggle', { 'wf-waiting': workflowsWait })}
            aria-pressed={workflowsOpen}
            title={
              workflowsWait
                ? 'An execution of a workflow is paused or waits for you'
                : 'The workflows of agents, and their executions in the folder of this window'
            }
            onClick={() => props.actions.toggleWorkflows()}
          >
            Workflows
          </button>
        </div>
      ) : null}
    </footer>
  );
}
