// The right panel of an execution: its facts and actions, and the node that the person selected
// on the canvas. A node that works shows the screen of its agent as text (`pane_text:<pane>`); a
// node with a report shows the report as the core renders it. A node that failed offers Retry and
// Skip, and a node whose agent ended offers "Continue in a tab".

import type { ExecutionValue, WorkflowGraph } from '@neohar/transport';
import { createEffect, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { useApp } from '../app/context';
import { RichHtml } from '../rich/RichHtml';
import { createProjectionStore } from '../stores';
import { executionWord, nodeTitle, runLine, stateWord, tone } from './model';

function Screen(props: { pane: string }) {
  const { transport } = useApp();
  const text = createProjectionStore(transport, `pane_text:${props.pane}`);
  onCleanup(() => text.close());
  return (
    <pre class="wf-screen" role="region" aria-label="The screen of the agent">
      {text.rows().join('\n')}
    </pre>
  );
}

export function ExecutionPanel(props: {
  execution: ExecutionValue;
  graph: WorkflowGraph;
  node: string | null;
}) {
  const { transport, run, report, windowId } = useApp();
  const [html, setHtml] = createSignal<string | null>(null);
  const [copied, setCopied] = createSignal<string | null>(null);
  const selected = () =>
    props.node ? props.execution.nodes.find((n) => n.node === props.node) : undefined;
  const spec = () => (props.node ? props.graph.nodes.find((n) => n.id === props.node) : undefined);
  // The report that the panel shows: the one of the node, else report.md.
  const reportKey = () => {
    const node = selected();
    if (node) return node.report ? `${props.execution.id}:${node.node}:${node.report}` : null;
    return props.execution.report ? `${props.execution.id}::${props.execution.report}` : null;
  };
  createEffect(
    on(reportKey, (key) => {
      setHtml(null);
      if (!key) return;
      const node = selected()?.node;
      transport
        .call('workflow.report', {
          execution: props.execution.id,
          ...(node ? { node } : {}),
        })
        .then((shown) => {
          if (reportKey() === key) setHtml(shown.markdown.html);
        })
        .catch(report);
    }),
  );
  const ended = () => props.execution.state === 'done' || props.execution.state === 'cancelled';

  return (
    <aside class="wf-panel" aria-label="Execution">
      <header class="wf-panel-head">
        <div class="wf-panel-title">
          <span class={`wf-dot tone-${tone(props.execution.state)}`} aria-hidden="true" />
          <strong>{props.execution.label}</strong>
          <span class="wf-muted">{executionWord(props.execution.state)}</span>
        </div>
        <p class="wf-muted">
          {props.execution.time_text}
          <Show when={props.execution.branch}>
            {(branch) => (
              <>
                {' · '}
                <code>{branch()}</code>
                <Show when={props.execution.head}>{(head) => <> @ {head()}</>}</Show>
              </>
            )}
          </Show>
          <Show when={props.execution.base}>{(base) => <> · base {base()}</>}</Show>
        </p>
        <Show when={props.execution.tree_changed}>
          <p class="wf-warn">The tree changed during the execution.</p>
        </Show>
        <div class="wf-panel-actions">
          <Show when={!ended()}>
            <button
              type="button"
              class="wf-danger"
              onClick={() => run('workflow.cancel', { execution: props.execution.id })}
            >
              Cancel
            </button>
          </Show>
          <button
            type="button"
            title={props.execution.dir}
            onClick={() =>
              run('fs.open', { window: windowId, path: props.execution.dir, reveal: true })
            }
          >
            Show in Finder
          </button>
          <button
            type="button"
            title="Copy the reports into workflow-reports/ of the folder"
            onClick={() =>
              transport
                .call('workflow.copy_to_folder', { execution: props.execution.id })
                .then((done) => setCopied(done.path))
                .catch(report)
            }
          >
            Copy to folder
          </button>
        </div>
        <Show when={copied()}>{(path) => <p class="wf-muted">Copied to {path()}</p>}</Show>
      </header>
      <Show
        when={selected()}
        fallback={
          <ul class="wf-panel-nodes">
            <For each={props.execution.nodes}>
              {(node) => {
                const found = props.graph.nodes.find((n) => n.id === node.node);
                return (
                  <li>
                    <span class={`wf-dot tone-${tone(node.state)}`} aria-hidden="true" />
                    <span>{found ? nodeTitle(found) : node.node}</span>
                    <span class="wf-muted">{runLine(node)}</span>
                  </li>
                );
              }}
            </For>
          </ul>
        }
      >
        {(node) => (
          <section class="wf-panel-node" aria-label="Node">
            <h3>
              {spec() ? nodeTitle(spec() as NonNullable<ReturnType<typeof spec>>) : node().node}
              <span class="wf-muted"> · {stateWord(node().state)}</span>
            </h3>
            <p class="wf-muted">{runLine(node())}</p>
            <Show when={node().reason}>{(reason) => <p class="wf-reason">{reason()}</p>}</Show>
            <div class="wf-panel-actions">
              <Show when={node().state === 'failed'}>
                <button
                  type="button"
                  class="primary"
                  onClick={() =>
                    run('workflow.retry', { execution: props.execution.id, node: node().node })
                  }
                >
                  Retry
                </button>
              </Show>
              <Show
                when={
                  (node().state === 'failed' ||
                    node().state === 'waiting' ||
                    node().state === 'queued') &&
                  !ended()
                }
              >
                <button
                  type="button"
                  onClick={() =>
                    run('workflow.skip', { execution: props.execution.id, node: node().node })
                  }
                >
                  Skip
                </button>
              </Show>
              <Show
                when={(node().state === 'working' || node().state === 'needs_you') && node().run}
              >
                <button
                  type="button"
                  title="Show the tab of the agent in the tab bar"
                  onClick={() =>
                    run('workflow.reveal', {
                      execution: props.execution.id,
                      node: node().node,
                      window: windowId,
                    })
                  }
                >
                  Open as tab
                </button>
              </Show>
              <Show when={node().resumable}>
                <button
                  type="button"
                  title="Start the agent again with its conversation, in a new tab"
                  onClick={() =>
                    run('workflow.continue', {
                      execution: props.execution.id,
                      node: node().node,
                      window: windowId,
                    })
                  }
                >
                  Continue in a tab
                </button>
              </Show>
            </div>
            <Show
              when={(node().state === 'working' || node().state === 'needs_you') && node().pane}
              keyed
            >
              {(pane) => <Screen pane={pane} />}
            </Show>
          </section>
        )}
      </Show>
      <Show when={html()}>
        {(shown) => (
          <div class="wf-report">
            <RichHtml html={shown()} />
          </div>
        )}
      </Show>
    </aside>
  );
}
