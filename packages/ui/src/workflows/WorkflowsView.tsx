// The Workflows view: a full view in the middle of the window, as the Git view is, where the
// person builds workflows of agents on a canvas, as in n8n, and executes them in the folder of
// the window. The left column lists the workflows and the executions; the middle is the canvas;
// the right panel shows an execution.
//
// The core keeps the files and the executions. `workflows` lists the files, and
// `workflow_executions` the executions with the state of each node. A change on the canvas is a
// save of the graph (`workflow.save`) with the revision that it starts from; the canvas draws the
// graph that the core gives back, with its errors. The saves go one after the other.

import {
  type AgentValue,
  type ExecutionValue,
  type HarnessChoices,
  RpcError,
  type WorkflowFacts,
  type WorkflowGraph,
  type WorkflowNode,
  type WorkflowNodePreset,
  type WorkflowPresets,
  type WorkflowShown,
} from '@neohar/transport';
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
} from 'solid-js';
import { useApp } from '../app/context';
import { windowFolder } from '../files/folder';
import { GitMenu, type MenuGroup } from '../git/Menu';
import { isAbsolutePath } from '../git/model';
import { createProjectionStore } from '../stores';
import { Canvas } from './Canvas';
import { ExecuteDialog } from './ExecuteDialog';
import { ExecutionPanel } from './ExecutionPanel';
import {
  addNode,
  connect,
  disconnect,
  executionWord,
  folderName,
  moveNode,
  type Point,
  problemsOf,
  removeNode,
  tone,
  updateNode,
} from './model';
import { NodeDetails } from './NodeDetails';
import { NodePicker } from './NodePicker';

interface Picking {
  screen: Point;
  at: Point;
  from?: string | undefined;
}

export function WorkflowsView() {
  const { stores, transport, report, windowId, view } = useApp();
  const workflows = createProjectionStore(transport, 'workflows');
  const executions = createProjectionStore(transport, 'workflow_executions');
  onCleanup(() => {
    workflows.close();
    executions.close();
  });
  let root: HTMLElement | undefined;

  const [folder, setFolder] = createSignal<string | null>(null);
  const [facts, setFacts] = createSignal<WorkflowFacts | null>(null);
  const [allFolders, setAllFolders] = createSignal(false);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [shown, setShown] = createSignal<WorkflowShown | null>(null);
  const [openError, setOpenError] = createSignal<string | null>(null);
  const [execution, setExecution] = createSignal<string | null>(null);
  const [executionGraph, setExecutionGraph] = createSignal<WorkflowGraph | null>(null);
  const [node, setNode] = createSignal<string | null>(null);
  const [details, setDetails] = createSignal<string | null>(null);
  const [picking, setPicking] = createSignal<Picking | null>(null);
  const [executing, setExecuting] = createSignal(false);
  const [presets, setPresets] = createSignal<WorkflowPresets | null>(null);
  const [choices, setChoices] = createSignal<HarnessChoices[]>([]);
  const [creating, setCreating] = createSignal(false);
  const [renaming, setRenaming] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal(false);

  // The folder of the request, else the folder of the window.
  createEffect(
    on(view.workflowsTarget, (target) => {
      setFolder(target.folder ?? windowFolder(stores, windowId) ?? null);
    }),
  );
  createEffect(
    on(folder, (path) => {
      setFacts(null);
      if (!path) return;
      transport
        .call('workflow.facts', { folder: path })
        .then((found) => {
          if (folder() === path) setFacts(found);
        })
        .catch(() => setFacts(null));
    }),
  );
  onMount(() => {
    root?.focus();
    transport.call('workflow.presets', {}).then(setPresets).catch(report);
    transport
      .call('agent.choices', {})
      .then((found) => setChoices(found.harnesses))
      .catch(() => setChoices([]));
  });

  // --- the workflow that the canvas edits ---------------------------------------------------

  const load = (name: string): void => {
    setOpenError(null);
    transport
      .call('workflow.show', { workflow: name })
      .then((found) => {
        if (selected() === name) setShown(found);
      })
      .catch((failure: unknown) => {
        setShown(null);
        setOpenError(failure instanceof RpcError ? failure.message : 'The workflow does not open.');
      });
  };
  const pick = (name: string): void => {
    setExecution(null);
    setNode(null);
    setSelected(name);
    load(name);
  };
  // The first workflow opens by itself.
  createEffect(() => {
    if (selected() === null && workflows.state.status === 'live') {
      const first = workflows.state.order[0];
      if (first) pick(first);
    }
  });
  // A file that changed by hand, or in another window, loads again.
  let saving = 0;
  createEffect(() => {
    const name = selected();
    const row = name ? workflows.state.byKey[name] : undefined;
    const current = shown();
    if (!name || !current || saving > 0) return;
    if (!row && workflows.state.status === 'live') {
      setShown(null);
      setSelected(null);
    } else if (row && row.revision !== current.revision && current.workflow === name) {
      load(name);
    }
  });

  let queue: Promise<unknown> = Promise.resolve();
  /** Saves a change of the graph after the saves before it, on the graph of the last answer. */
  const edit = (change: (graph: WorkflowGraph) => WorkflowGraph | null): void => {
    saving += 1;
    queue = queue
      .then(() => {
        const current = shown();
        if (!current) return undefined;
        const next = change(current.graph);
        if (!next) return undefined;
        return transport
          .call('workflow.save', {
            workflow: current.workflow,
            revision: current.revision,
            graph: next,
          })
          .then(setShown);
      })
      .catch((failure: unknown) => {
        const name = selected();
        if (failure instanceof RpcError && failure.code === 'conflict' && name) load(name);
        report(failure);
      })
      .finally(() => {
        saving -= 1;
      });
  };
  const step = (method: 'workflow.undo' | 'workflow.redo' | 'workflow.layout'): void => {
    const current = shown();
    if (!current) return;
    saving += 1;
    queue = queue
      .then(() => {
        const latest = shown() ?? current;
        return (
          method === 'workflow.layout'
            ? transport.call(method, { workflow: latest.workflow, revision: latest.revision })
            : transport.call(method, { workflow: latest.workflow })
        ).then(setShown);
      })
      .catch(report)
      .finally(() => {
        saving -= 1;
      });
  };

  const addPreset = (preset: WorkflowNodePreset): void => {
    const at = picking();
    setPicking(null);
    if (!at) return;
    let made: string | null = null;
    edit((graph) => {
      const added = addNode(graph, preset, at.at, at.from);
      made = added.id;
      return added.graph;
    });
    queue = queue.then(() => {
      if (made) setNode(made);
    });
  };

  // --- executions -----------------------------------------------------------------------------

  const rows = createMemo(() =>
    executions.rows().filter((row) => allFolders() || !folder() || row.folder === folder()),
  );
  const current = (): ExecutionValue | undefined => {
    const id = execution();
    return id ? executions.state.byKey[id] : undefined;
  };
  const openExecution = (id: string): void => {
    setExecution(id);
    setNode(null);
    setExecutionGraph(null);
    transport
      .call('workflow.execution', { execution: id })
      .then((found) => {
        if (execution() === id) setExecutionGraph(found.graph);
      })
      .catch(report);
  };
  /** The last execution of the workflow in the folder, for the reports of before. */
  const last = () =>
    executions
      .rows()
      .find(
        (row) => row.workflow === selected() && row.folder === folder() && row.state !== 'running',
      );

  const execute = (base: string | null, notes: string | null, only?: string): void => {
    const name = selected();
    const path = folder();
    setExecuting(false);
    if (!name || !path) return;
    transport
      .call('workflow.execute', {
        workflow: name,
        folder: path,
        ...(base ? { base } : {}),
        ...(notes ? { notes } : {}),
        ...(only ? { node: only } : {}),
      })
      .then((started) => openExecution(started.execution))
      .catch(report);
  };

  // --- the bar --------------------------------------------------------------------------------

  const folderGroups = (): MenuGroup[] => {
    const seen = new Set<string>();
    const items: MenuGroup['items'] = [];
    const add = (path: string | null | undefined, detail?: string): void => {
      if (!path || seen.has(path)) return;
      seen.add(path);
      items.push({
        key: path,
        title: folderName(path),
        detail: detail ?? path,
        current: path === folder(),
      });
    };
    add(folder());
    for (const pane of stores.panes.rows()) add(pane.cwd);
    for (const workspace of stores.tree.rows()) add(workspace.root);
    return [{ items }];
  };
  const agents = (): AgentValue[] => stores.agents.rows();
  const paused = () => rows().some((row) => row.state === 'paused');

  const graph = (): WorkflowGraph | null =>
    execution() ? executionGraph() : (shown()?.graph ?? null);
  const detailsNode = (): WorkflowNode | undefined => {
    const id = details();
    return id ? shown()?.graph.nodes.find((n) => n.id === id) : undefined;
  };
  const problems = () => shown()?.problems ?? [];
  const canExecute = () => !!shown() && problems().length === 0 && !!folder();

  const onKey = (event: KeyboardEvent): void => {
    if (event.defaultPrevented) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      if (picking()) setPicking(null);
      else if (node()) setNode(null);
      else view.closeWorkflows();
      return;
    }
    const mod = event.metaKey || event.ctrlKey;
    if (mod && event.key.toLowerCase() === 'z' && !execution()) {
      event.preventDefault();
      step(event.shiftKey ? 'workflow.redo' : 'workflow.undo');
    }
  };

  return (
    <section
      class="wf-view"
      aria-label="Workflows view"
      tabIndex={-1}
      ref={(element) => (root = element)}
      onKeyDown={onKey}
    >
      <header class="wf-bar">
        <GitMenu
          label="Folder"
          value={folder() ? folderName(folder() as string) : 'No folder'}
          note={facts()?.branch ?? null}
          groups={folderGroups()}
          onPick={setFolder}
          actions={(filter) =>
            isAbsolutePath(filter)
              ? [{ label: `Use ${filter.trim()}`, run: () => setFolder(filter.trim()) }]
              : []
          }
          empty="Type the absolute path of a folder."
        />
        <Show when={shown()}>
          {(current) => (
            <Show
              when={renaming()}
              fallback={
                <button
                  type="button"
                  class="wf-name"
                  title="Rename the workflow"
                  onClick={() => setRenaming(true)}
                >
                  {current().graph.name}
                </button>
              }
            >
              <input
                class="wf-name-field"
                type="text"
                autocomplete="off"
                spellcheck={false}
                value={current().graph.name}
                ref={(element) => queueMicrotask(() => element.select())}
                onKeyDown={(event) => {
                  event.stopPropagation();
                  if (event.key === 'Escape') setRenaming(false);
                  if (event.key === 'Enter') event.currentTarget.blur();
                }}
                onBlur={(event) => {
                  const name = event.currentTarget.value.trim();
                  setRenaming(false);
                  if (!name || name === current().graph.name) return;
                  transport
                    .call('workflow.rename', { workflow: current().workflow, name })
                    .then((renamed) => {
                      setSelected(renamed.workflow);
                      setShown(renamed);
                    })
                    .catch(report);
                }}
              />
            </Show>
          )}
        </Show>
        <span class="wf-spacer" />
        <Show when={shown() && !execution()}>
          <button
            type="button"
            title="Undo (⌘Z)"
            disabled={!shown()?.can_undo}
            onClick={() => step('workflow.undo')}
          >
            Undo
          </button>
          <button
            type="button"
            title="Redo (⌘⇧Z)"
            disabled={!shown()?.can_redo}
            onClick={() => step('workflow.redo')}
          >
            Redo
          </button>
        </Show>
        <Show when={execution()}>
          <button type="button" onClick={() => setExecution(null)}>
            Back to the workflow
          </button>
        </Show>
        <button
          type="button"
          class="wf-execute-button"
          disabled={!canExecute()}
          title={
            problems().length > 0
              ? 'The workflow has errors'
              : folder()
                ? `Execute in ${folder()}`
                : 'Pick a folder first'
          }
          onClick={() => setExecuting(true)}
        >
          ▶ Execute workflow
        </button>
        <button
          type="button"
          class="git-close"
          title="Close the Workflows view (Esc)"
          onClick={() => view.closeWorkflows()}
        >
          Close
        </button>
      </header>
      <div class="wf-body" classList={{ 'with-panel': !!current() }}>
        <nav class="wf-side" aria-label="Workflows and executions">
          <h3 class="wf-side-title">Workflows</h3>
          <ul class="wf-list">
            <For each={workflows.rows()}>
              {(row) => (
                <li>
                  <button
                    type="button"
                    class="wf-list-item"
                    classList={{ selected: row.workflow === selected() && !execution() }}
                    onClick={() => pick(row.workflow)}
                  >
                    <span class="wf-list-name">{row.name}</span>
                    <Show when={row.problems > 0}>
                      <span class="wf-badge danger" title="The workflow has errors">
                        {row.problems}
                      </span>
                    </Show>
                  </button>
                </li>
              )}
            </For>
          </ul>
          <Show
            when={creating()}
            fallback={
              <div class="wf-side-actions">
                <button type="button" class="wf-link" onClick={() => setCreating(true)}>
                  + New
                </button>
                <Show when={shown()}>
                  <button
                    type="button"
                    class="wf-link"
                    onClick={() =>
                      transport
                        .call('workflow.duplicate', { workflow: shown()?.workflow ?? '' })
                        .then((copy) => pick(copy.workflow))
                        .catch(report)
                    }
                  >
                    Duplicate
                  </button>
                  <Show
                    when={confirmDelete()}
                    fallback={
                      <button type="button" class="wf-link" onClick={() => setConfirmDelete(true)}>
                        Delete
                      </button>
                    }
                  >
                    <button
                      type="button"
                      class="wf-link danger"
                      onClick={() => {
                        setConfirmDelete(false);
                        const name = shown()?.workflow;
                        if (!name) return;
                        transport
                          .call('workflow.delete', { workflow: name })
                          .then(() => {
                            setShown(null);
                            setSelected(null);
                          })
                          .catch(report);
                      }}
                    >
                      Delete it
                    </button>
                  </Show>
                </Show>
              </div>
            }
          >
            <form
              class="wf-new"
              onSubmit={(event) => {
                event.preventDefault();
                const form = event.currentTarget;
                const name = (form.elements.namedItem('name') as HTMLInputElement).value.trim();
                const preset = (form.elements.namedItem('preset') as HTMLSelectElement).value;
                setCreating(false);
                if (!name) return;
                transport
                  .call('workflow.create', { name, ...(preset ? { preset } : {}) })
                  .then((made) => pick(made.workflow))
                  .catch(report);
              }}
            >
              <input
                name="name"
                type="text"
                placeholder="Name"
                autocomplete="off"
                ref={(element) => queueMicrotask(() => element.focus())}
                onKeyDown={(event) => {
                  event.stopPropagation();
                  if (event.key === 'Escape') setCreating(false);
                }}
              />
              <select name="preset">
                <option value="">Only the trigger</option>
                <For each={presets()?.workflows ?? []}>
                  {(preset) => <option value={preset.id}>{preset.label}</option>}
                </For>
              </select>
              <div class="wf-new-buttons">
                <button type="button" class="wf-link" onClick={() => setCreating(false)}>
                  Cancel
                </button>
                <button type="submit" class="primary">
                  Create
                </button>
              </div>
            </form>
          </Show>
          <h3 class="wf-side-title">
            Executions
            <button
              type="button"
              class="wf-link wf-side-toggle"
              aria-pressed={allFolders()}
              onClick={() => setAllFolders((all) => !all)}
            >
              {allFolders() ? 'All folders' : 'This folder'}
            </button>
          </h3>
          <ul class="wf-list">
            <For each={rows()}>
              {(row) => (
                <li>
                  <button
                    type="button"
                    class="wf-list-item wf-execution"
                    classList={{ selected: row.id === execution() }}
                    title={row.folder}
                    onClick={() => openExecution(row.id)}
                  >
                    <span class={`wf-dot tone-${tone(row.state)}`} aria-hidden="true" />
                    <span class="wf-list-name">{row.label}</span>
                    <span class="wf-muted">
                      {row.state === 'done' ? row.time_text : executionWord(row.state)}
                    </span>
                  </button>
                </li>
              )}
            </For>
            <Show when={rows().length === 0}>
              <li class="wf-muted wf-empty">
                No execution{allFolders() ? '' : ' in this folder'}.
              </li>
            </Show>
          </ul>
          <Show when={paused()}>
            <p class="wf-warn">An execution is paused: a node failed.</p>
          </Show>
        </nav>
        <div class="wf-main">
          <Show when={openError()}>
            {(message) => (
              <p class="git-error git-banner" role="alert">
                {message()}
              </p>
            )}
          </Show>
          <Show when={!execution() && problemsOf(problems(), null)[0]}>
            {(problem) => (
              <p class="git-note git-banner" role="status">
                {problem().text}
              </p>
            )}
          </Show>
          <Show
            when={graph()}
            fallback={
              <p class="git-empty git-center">
                {workflows.rows().length === 0
                  ? 'No workflow yet. Click + New to make one, for example from the Review preset.'
                  : 'Pick a workflow.'}
              </p>
            }
          >
            {(shownGraph) => (
              <Canvas
                graph={shownGraph()}
                fitKey={execution() ?? selected() ?? ''}
                problems={execution() ? [] : problems()}
                execution={current()}
                base={execution() ? current()?.base : facts()?.base}
                selected={node()}
                onSelect={setNode}
                onOpen={(id) => {
                  if (!execution()) setDetails(id);
                }}
                onMove={(id, at) => edit((g) => moveNode(g, id, at))}
                onConnect={(from, to) => edit((g) => connect(g, from, to))}
                onDisconnect={(from, to) => edit((g) => disconnect(g, from, to))}
                onDelete={(id) => {
                  setNode(null);
                  edit((g) => removeNode(g, id));
                }}
                onPick={(screen, at, from) => setPicking({ screen, at, from })}
                onTidy={() => step('workflow.layout')}
              />
            )}
          </Show>
        </div>
        <Show when={current()}>
          {(found) => (
            <Show when={executionGraph()}>
              {(executed) => (
                <ExecutionPanel execution={found()} graph={executed()} node={node()} />
              )}
            </Show>
          )}
        </Show>
      </div>
      <Show when={picking()}>
        {(at) => (
          <NodePicker
            at={at().screen}
            presets={presets()?.nodes ?? []}
            onPick={addPreset}
            onClose={() => setPicking(null)}
          />
        )}
      </Show>
      <Show when={details()} keyed>
        {(id) => (
          <Show when={detailsNode()}>
            {(found) => (
              <NodeDetails
                graph={shown()?.graph ?? { name: '', nodes: [] }}
                node={found()}
                problems={problems()}
                choices={choices()}
                last={last()}
                onChange={(patch) => edit((g) => updateNode(g, id, patch))}
                onExecuteNode={() => {
                  setDetails(null);
                  execute(null, null, id);
                }}
                onClose={() => setDetails(null)}
              />
            )}
          </Show>
        )}
      </Show>
      <Show when={executing() && shown()}>
        {(current) => (
          <ExecuteDialog
            workflow={current().workflow}
            name={current().graph.name}
            folder={folder() ?? ''}
            facts={facts()}
            agents={agents()}
            onExecute={(base, notes) => execute(base, notes)}
            onCancel={() => setExecuting(false)}
          />
        )}
      </Show>
    </section>
  );
}
