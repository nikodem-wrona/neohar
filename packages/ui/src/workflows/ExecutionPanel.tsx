// The right panel of an execution: its facts and actions, and the node that the person selected
// on the canvas. A node that works shows the screen of its agent as text (`pane_text:<pane>`); a
// node with a report shows the report as the core renders it. A node that failed offers Retry and
// Skip, and a node whose agent ended offers "Continue in a tab".

import type { ExecutionValue, WorkflowGraph } from '@neohar/transport';
import { useEffect, useState } from 'react';
import { useApp } from '../app/context';
import { useLatest, useRows } from '../lib/hooks';
import { RichHtml } from '../rich/RichHtml';
import { useProjectionStore } from '../stores';
import { executionWord, nodeTitle, runLine, stateWord, tone } from './model';

function Screen(props: { pane: string }) {
  const { transport } = useApp();
  const text = useProjectionStore(transport, `pane_text:${props.pane}`);
  const rows = useRows(text);
  return (
    <pre className="wf-screen" role="region" aria-label="The screen of the agent">
      {rows.join('\n')}
    </pre>
  );
}

export function ExecutionPanel(props: {
  execution: ExecutionValue;
  graph: WorkflowGraph;
  node: string | null;
}) {
  const { transport, run, report, windowId } = useApp();
  const [html, setHtml] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const selected = props.node
    ? props.execution.nodes.find((n) => n.node === props.node)
    : undefined;
  const spec = props.node ? props.graph.nodes.find((n) => n.id === props.node) : undefined;
  // The report that the panel shows: the one of the node, else report.md.
  const reportKey = selected
    ? selected.report
      ? `${props.execution.id}:${selected.node}:${selected.report}`
      : null
    : props.execution.report
      ? `${props.execution.id}::${props.execution.report}`
      : null;
  const latestKey = useLatest(reportKey);
  const execution = props.execution.id;
  const reportNode = selected?.node;
  // The key names the execution, the node and the report: a new key asks for the report again.
  useEffect(() => {
    setHtml(null);
    if (!reportKey) return;
    transport
      .call('workflow.report', {
        execution,
        ...(reportNode ? { node: reportNode } : {}),
      })
      .then((shown) => {
        if (latestKey.current === reportKey) setHtml(shown.markdown.html);
      })
      .catch(report);
  }, [reportKey, transport, execution, reportNode, latestKey, report]);
  const ended = props.execution.state === 'done' || props.execution.state === 'cancelled';
  const screen =
    selected && (selected.state === 'working' || selected.state === 'needs_you')
      ? selected.pane
      : null;

  return (
    <aside className="wf-panel" aria-label="Execution">
      <header className="wf-panel-head">
        <div className="wf-panel-title">
          <span className={`wf-dot tone-${tone(props.execution.state)}`} aria-hidden="true" />
          <strong>{props.execution.label}</strong>
          <span className="wf-muted">{executionWord(props.execution.state)}</span>
        </div>
        <p className="wf-muted">
          {props.execution.time_text}
          {props.execution.branch ? (
            <>
              {' · '}
              <code>{props.execution.branch}</code>
              {props.execution.head ? <> @ {props.execution.head}</> : null}
            </>
          ) : null}
          {props.execution.base ? <> · base {props.execution.base}</> : null}
        </p>
        {props.execution.tree_changed ? (
          <p className="wf-warn">The tree changed during the execution.</p>
        ) : null}
        <div className="wf-panel-actions">
          {!ended ? (
            <button
              type="button"
              className="wf-danger"
              onClick={() => run('workflow.cancel', { execution: props.execution.id })}
            >
              Cancel
            </button>
          ) : null}
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
        {copied ? <p className="wf-muted">Copied to {copied}</p> : null}
      </header>
      {selected ? (
        <section className="wf-panel-node" aria-label="Node">
          <h3>
            {spec ? nodeTitle(spec) : selected.node}
            <span className="wf-muted"> · {stateWord(selected.state)}</span>
          </h3>
          <p className="wf-muted">{runLine(selected)}</p>
          {selected.reason ? <p className="wf-reason">{selected.reason}</p> : null}
          <div className="wf-panel-actions">
            {selected.state === 'failed' ? (
              <button
                type="button"
                className="primary"
                onClick={() =>
                  run('workflow.retry', { execution: props.execution.id, node: selected.node })
                }
              >
                Retry
              </button>
            ) : null}
            {(selected.state === 'failed' ||
              selected.state === 'waiting' ||
              selected.state === 'queued') &&
            !ended ? (
              <button
                type="button"
                onClick={() =>
                  run('workflow.skip', { execution: props.execution.id, node: selected.node })
                }
              >
                Skip
              </button>
            ) : null}
            {(selected.state === 'working' || selected.state === 'needs_you') && selected.run ? (
              <button
                type="button"
                title="Show the tab of the agent in the tab bar"
                onClick={() =>
                  run('workflow.reveal', {
                    execution: props.execution.id,
                    node: selected.node,
                    window: windowId,
                  })
                }
              >
                Open as tab
              </button>
            ) : null}
            {selected.resumable ? (
              <button
                type="button"
                title="Start the agent again with its conversation, in a new tab"
                onClick={() =>
                  run('workflow.continue', {
                    execution: props.execution.id,
                    node: selected.node,
                    window: windowId,
                  })
                }
              >
                Continue in a tab
              </button>
            ) : null}
          </div>
          {screen ? <Screen key={screen} pane={screen} /> : null}
        </section>
      ) : (
        <ul className="wf-panel-nodes">
          {props.execution.nodes.map((node) => {
            const found = props.graph.nodes.find((n) => n.id === node.node);
            return (
              <li key={node.node}>
                <span className={`wf-dot tone-${tone(node.state)}`} aria-hidden="true" />
                <span>{found ? nodeTitle(found) : node.node}</span>
                <span className="wf-muted">{runLine(node)}</span>
              </li>
            );
          })}
        </ul>
      )}
      {html ? (
        <div className="wf-report">
          <RichHtml html={html} />
        </div>
      ) : null}
    </aside>
  );
}
