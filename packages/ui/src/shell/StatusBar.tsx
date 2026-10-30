// The status bar: the function keys on the left, then the limits of the harnesses, the warnings of
// the core, the renderer when it fell back to `canvas2d`, the machine, and, after a rule, the
// switches of the Git view and of the Workflows view as one group. The left column and the drawer
// fold and open from their own arrows and rails. A key cap shows the chord of the settings and runs
// the same command when it is clicked. The caps are gray, and the cap of Answer is amber while an
// agent waits.
// The bar shows what the projections say and decides nothing.

import { For, onCleanup, Show } from 'solid-js';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { hasUsage, limitText, usageText } from '../timeline/model';
import { createProjectionStore } from '../stores';
import { needsYou } from '../workflows/model';
import { chordText, FUNCTION_KEYS } from './keys';

export function StatusBar(props: { actions: ActionContext; keymap: () => Keymap }) {
  const { stores, view, platform, form, transport } = useApp();
  // The switch of the Workflows view has a mark while an execution is paused or waits for you.
  const executions = createProjectionStore(transport, 'workflow_executions');
  onCleanup(() => executions.close());
  const workflowsWait = () => executions.rows().some(needsYou);
  // The host serves `link` to each window. Without a host (the dev WebSocket) the window is local.
  const machine = () => stores.link.state.byKey['_']?.text ?? 'local';
  const health = () => stores.health.state.byKey['_'];
  const warnings = () => health()?.warnings ?? [];
  const renderer = () => health()?.renderer;
  const waiting = () => stores.attention.state.order.length > 0;
  return (
    <footer class="statusbar">
      <div class="fkeys" role="toolbar" aria-label="Function keys">
        <For each={FUNCTION_KEYS}>
          {(key) => (
            <Show when={chordText(props.keymap(), key.command, platform)}>
              {(chord) => (
                <button
                  type="button"
                  class="fkey"
                  classList={{ optional: key.optional === true }}
                  data-command={key.command}
                  onClick={() => runCommand(key.command, props.actions)}
                >
                  <kbd class="cap" classList={{ 'cap-wait': key.waits === true && waiting() }}>
                    {chord()}
                  </kbd>
                  {key.label}
                </button>
              )}
            </Show>
          )}
        </For>
      </div>
      <Show when={view.passNext()}>
        <span class="status-pass" role="status">
          Next key goes to the pane
        </span>
      </Show>
      <span class="status-spacer" />
      <For each={stores.limits.rows()}>
        {(limit) => (
          <Show when={limitText(limit, harnessName(limit.harness))}>
            {(text) => (
              <span
                class="status-limit"
                title={
                  hasUsage(limit.usage_today) ? `Today: ${usageText(limit.usage_today)}` : undefined
                }
              >
                {text()}
              </span>
            )}
          </Show>
        )}
      </For>
      <Show when={warnings().length > 0}>
        <span
          class="status-warnings"
          title={warnings()
            .map((w) => w.text)
            .join('\n')}
        >
          {warnings().length} {warnings().length === 1 ? 'warning' : 'warnings'}
        </span>
      </Show>
      <Show when={renderer() !== 'webgl2'}>
        <span class="status-renderer">{renderer() ?? 'no renderer'}</span>
      </Show>
      <span class="status-machine">{machine()}</span>
      <Show when={form !== 'phone'}>
        <div class="status-views" role="group" aria-label="Views">
          <button
            type="button"
            class="drawer-toggle"
            aria-pressed={view.gitOpen()}
            title="The Git view of the folder of this window"
            onClick={() => props.actions.toggleGit()}
          >
            Git
          </button>
          <button
            type="button"
            class="drawer-toggle"
            classList={{ 'wf-waiting': workflowsWait() }}
            aria-pressed={view.workflowsOpen()}
            title={
              workflowsWait()
                ? 'An execution of a workflow is paused or waits for you'
                : 'The workflows of agents, and their executions in the folder of this window'
            }
            onClick={() => props.actions.toggleWorkflows()}
          >
            Workflows
          </button>
        </div>
      </Show>
    </footer>
  );
}
