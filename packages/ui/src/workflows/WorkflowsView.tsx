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
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import { ScrollArea } from '../components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { windowFolder } from '../files/folder';
import { GitMenu, type MenuGroup } from '../git/Menu';
import { isAbsolutePath } from '../git/model';
import { useRows, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { createValue } from '../lib/value';
import { useProjectionStore } from '../stores';
import { Canvas } from './Canvas';
import { ExecuteDialog } from './ExecuteDialog';
import { ExecutionPanel } from './ExecutionPanel';
import { NONE } from './fields';
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

/** Selects the text of the field when it shows. */
function selectWhenShown(element: HTMLInputElement | null): void {
  if (element) queueMicrotask(() => element.select());
}

/** Gives the focus to the field when it shows. */
function focusWhenShown(element: HTMLInputElement | null): void {
  if (element) queueMicrotask(() => element.focus());
}

export function WorkflowsView() {
  const { stores, transport, report, windowId, view } = useApp();
  const workflows = useProjectionStore(transport, 'workflows');
  const executions = useProjectionStore(transport, 'workflow_executions');
  const root = useRef<HTMLElement>(null);

  // The state that the answers of the core and the saves read at once, before the next render
  // shows it. The folder of the request, else the folder of the window, from the start.
  const [live] = useState(() => ({
    folder: createValue<string | null>(
      view.workflowsTarget.get().folder ?? windowFolder(stores, windowId) ?? null,
    ),
    selected: createValue<string | null>(null),
    shown: createValue<WorkflowShown | null>(null),
    execution: createValue<string | null>(null),
  }));
  const folder = useValue(live.folder);
  const selected = useValue(live.selected);
  const shown = useValue(live.shown);
  const execution = useValue(live.execution);
  const [facts, setFacts] = useState<WorkflowFacts | null>(null);
  const [allFolders, setAllFolders] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const [executionGraph, setExecutionGraph] = useState<WorkflowGraph | null>(null);
  const [node, setNode] = useState<string | null>(null);
  const [details, setDetails] = useState<string | null>(null);
  const [picking, setPicking] = useState<Picking | null>(null);
  const [executing, setExecuting] = useState(false);
  const [presets, setPresets] = useState<WorkflowPresets | null>(null);
  const [choices, setChoices] = useState<HarnessChoices[]>([]);
  const [creating, setCreating] = useState(false);
  // The preset of a new workflow, and whether its list is open. `''` is no preset.
  const [newPreset, setNewPreset] = useState('');
  const [presetOpen, setPresetOpen] = useState(false);
  useLayer(presetOpen);
  const [renaming, setRenaming] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  // The folder of the request, else the folder of the window.
  const target = useValue(view.workflowsTarget);
  useEffect(() => {
    live.folder.set(target.folder ?? windowFolder(stores, windowId) ?? null);
  }, [target, live, stores, windowId]);
  useEffect(() => {
    setFacts(null);
    if (!folder) return;
    transport
      .call('workflow.facts', { folder })
      .then((found) => {
        if (live.folder.get() === folder) setFacts(found);
      })
      .catch(() => setFacts(null));
  }, [folder, transport, live]);
  useEffect(() => {
    root.current?.focus();
    transport.call('workflow.presets', {}).then(setPresets).catch(report);
    transport
      .call('agent.choices', {})
      .then((found) => setChoices(found.harnesses))
      .catch(() => setChoices([]));
  }, [transport, report]);

  // --- the workflow that the canvas edits ---------------------------------------------------

  const load = useCallback(
    (name: string): void => {
      setOpenError(null);
      transport
        .call('workflow.show', { workflow: name })
        .then((found) => {
          if (live.selected.get() === name) live.shown.set(found);
        })
        .catch((failure: unknown) => {
          live.shown.set(null);
          setOpenError(
            failure instanceof RpcError ? failure.message : 'The workflow does not open.',
          );
        });
    },
    [transport, live],
  );
  const pick = useCallback(
    (name: string): void => {
      live.execution.set(null);
      setNode(null);
      live.selected.set(name);
      load(name);
    },
    [live, load],
  );
  // The first workflow opens by itself.
  const listed = useValue(workflows);
  const workflowRows = useRows(workflows);
  const firstWorkflow = listed.order[0];
  useEffect(() => {
    if (selected === null && listed.status === 'live') {
      if (firstWorkflow) pick(firstWorkflow);
    }
  }, [selected, listed.status, firstWorkflow, pick]);
  // A file that changed by hand, or in another window, loads again. The check runs at once after
  // each change of the workflow, of its row or of the graph, so that a save in progress is seen.
  const saving = useRef(0);
  useEffect(() => {
    let seen: { row: unknown; status: string } | null = null;
    const check = (): void => {
      const name = live.selected.get();
      const state = workflows.get();
      const row = name ? state.byKey[name] : undefined;
      const current = live.shown.get();
      seen = { row, status: state.status };
      if (!name || !current || saving.current > 0) return;
      if (!row && state.status === 'live') {
        live.shown.set(null);
        live.selected.set(null);
      } else if (row && row.revision !== current.revision && current.workflow === name) {
        load(name);
      }
    };
    const onWorkflows = (): void => {
      const name = live.selected.get();
      const state = workflows.get();
      const row = name ? state.byKey[name] : undefined;
      if (seen && seen.row === row && seen.status === state.status) return;
      check();
    };
    check();
    const stops = [
      live.selected.subscribe(check),
      live.shown.subscribe(check),
      workflows.subscribe(onWorkflows),
    ];
    return () => {
      for (const stop of stops) stop();
    };
  }, [live, workflows, load]);

  const queue = useRef<Promise<unknown>>(Promise.resolve());
  /** Saves a change of the graph after the saves before it, on the graph of the last answer. */
  const edit = (change: (graph: WorkflowGraph) => WorkflowGraph | null): void => {
    saving.current += 1;
    queue.current = queue.current
      .then(() => {
        const current = live.shown.get();
        if (!current) return undefined;
        const next = change(current.graph);
        if (!next) return undefined;
        return transport
          .call('workflow.save', {
            workflow: current.workflow,
            revision: current.revision,
            graph: next,
          })
          .then((found) => live.shown.set(found));
      })
      .catch((failure: unknown) => {
        const name = live.selected.get();
        if (failure instanceof RpcError && failure.code === 'conflict' && name) load(name);
        report(failure);
      })
      .finally(() => {
        saving.current -= 1;
      });
  };
  const step = (method: 'workflow.undo' | 'workflow.redo' | 'workflow.layout'): void => {
    const current = live.shown.get();
    if (!current) return;
    saving.current += 1;
    queue.current = queue.current
      .then(() => {
        const latest = live.shown.get() ?? current;
        return (
          method === 'workflow.layout'
            ? transport.call(method, { workflow: latest.workflow, revision: latest.revision })
            : transport.call(method, { workflow: latest.workflow })
        ).then((found) => live.shown.set(found));
      })
      .catch(report)
      .finally(() => {
        saving.current -= 1;
      });
  };

  const addPreset = (preset: WorkflowNodePreset): void => {
    const at = picking;
    setPicking(null);
    if (!at) return;
    let made: string | null = null;
    edit((graph) => {
      const added = addNode(graph, preset, at.at, at.from);
      made = added.id;
      return added.graph;
    });
    queue.current = queue.current.then(() => {
      if (made) setNode(made);
    });
  };

  // --- executions -----------------------------------------------------------------------------

  const executionRows = useRows(executions);
  const executionState = useValue(executions);
  const rows = executionRows.filter((row) => allFolders || !folder || row.folder === folder);
  const current: ExecutionValue | undefined = execution
    ? executionState.byKey[execution]
    : undefined;
  const openExecution = (id: string): void => {
    live.execution.set(id);
    setNode(null);
    setExecutionGraph(null);
    transport
      .call('workflow.execution', { execution: id })
      .then((found) => {
        if (live.execution.get() === id) setExecutionGraph(found.graph);
      })
      .catch(report);
  };
  /** The last execution of the workflow in the folder, for the reports of before. */
  const last = executionRows.find(
    (row) => row.workflow === selected && row.folder === folder && row.state !== 'running',
  );

  const execute = (base: string | null, notes: string | null, only?: string): void => {
    const name = live.selected.get();
    const path = live.folder.get();
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

  const panes = useRows(stores.panes);
  const tree = useRows(stores.tree);
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
        current: path === folder,
      });
    };
    add(folder);
    for (const pane of panes) add(pane.cwd);
    for (const workspace of tree) add(workspace.root);
    return [{ items }];
  };
  const agents: AgentValue[] = useRows(stores.agents);
  const paused = rows.some((row) => row.state === 'paused');

  const graph: WorkflowGraph | null = execution ? executionGraph : (shown?.graph ?? null);
  const detailsNode: WorkflowNode | undefined = details
    ? shown?.graph.nodes.find((n) => n.id === details)
    : undefined;
  const problems = shown?.problems ?? [];
  const canExecute = !!shown && problems.length === 0 && !!folder;
  const banner = !execution ? problemsOf(problems, null)[0] : undefined;

  const onKey = (event: KeyboardEvent): void => {
    if (event.defaultPrevented) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      if (picking) setPicking(null);
      else if (node) setNode(null);
      else view.closeWorkflows();
      return;
    }
    const mod = event.metaKey || event.ctrlKey;
    if (mod && event.key.toLowerCase() === 'z' && !execution) {
      event.preventDefault();
      step(event.shiftKey ? 'workflow.redo' : 'workflow.undo');
    }
  };

  return (
    <section
      className="wf-view"
      aria-label="Workflows view"
      tabIndex={-1}
      ref={root}
      onKeyDown={onKey}
    >
      <header className="wf-bar">
        <GitMenu
          label="Folder"
          value={folder ? folderName(folder) : 'No folder'}
          note={facts?.branch ?? null}
          groups={folderGroups()}
          onPick={(path) => live.folder.set(path)}
          actions={(filter) =>
            isAbsolutePath(filter)
              ? [{ label: `Use ${filter.trim()}`, run: () => live.folder.set(filter.trim()) }]
              : []
          }
          empty="Type the absolute path of a folder."
        />
        {shown ? (
          renaming ? (
            <Input
              className="h-7 w-[28ch] px-2 py-0"
              aria-label="The name of the workflow"
              type="text"
              autoComplete="off"
              spellCheck={false}
              defaultValue={shown.graph.name}
              ref={selectWhenShown}
              onKeyDown={(event) => {
                event.stopPropagation();
                if (event.key === 'Escape') setRenaming(false);
                if (event.key === 'Enter') event.currentTarget.blur();
              }}
              onBlur={(event) => {
                const name = event.currentTarget.value.trim();
                setRenaming(false);
                if (!name || name === shown.graph.name) return;
                transport
                  .call('workflow.rename', { workflow: shown.workflow, name })
                  .then((renamed) => {
                    live.selected.set(renamed.workflow);
                    live.shown.set(renamed);
                  })
                  .catch(report);
              }}
            />
          ) : (
            <Button
              variant="ghost"
              size="xs"
              className="max-w-[32ch] font-semibold"
              title="Rename the workflow"
              onClick={() => setRenaming(true)}
            >
              <span className="truncate">{shown.graph.name}</span>
            </Button>
          )
        ) : null}
        <span className="wf-spacer" />
        {shown && !execution ? (
          <>
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              title="Undo (⌘Z)"
              disabled={!shown.can_undo}
              onClick={() => step('workflow.undo')}
            >
              Undo
            </Button>
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              title="Redo (⌘⇧Z)"
              disabled={!shown.can_redo}
              onClick={() => step('workflow.redo')}
            >
              Redo
            </Button>
          </>
        ) : null}
        {execution ? (
          <Button
            variant="ghost"
            size="xs"
            className="text-muted-foreground"
            onClick={() => live.execution.set(null)}
          >
            Back to the workflow
          </Button>
        ) : null}
        <Button
          size="xs"
          className="font-semibold"
          disabled={!canExecute}
          title={
            problems.length > 0
              ? 'The workflow has errors'
              : folder
                ? `Execute in ${folder}`
                : 'Pick a folder first'
          }
          onClick={() => setExecuting(true)}
        >
          ▶ Execute workflow
        </Button>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          title="Close the Workflows view (Esc)"
          onClick={() => view.closeWorkflows()}
        >
          Close
        </Button>
      </header>
      <div className={cn('wf-body', { 'with-panel': !!current })}>
        <nav className="wf-side" aria-label="Workflows and executions">
          <ScrollArea className="min-h-0 flex-1">
            <div className="py-(--s2)">
              <h3 className="wf-side-title">Workflows</h3>
              <ul className="wf-list">
                {workflowRows.map((row) => (
                  <li key={row.workflow}>
                    <button
                      type="button"
                      className={cn('wf-list-item', {
                        selected: row.workflow === selected && !execution,
                      })}
                      onClick={() => pick(row.workflow)}
                    >
                      <span className="wf-list-name">{row.name}</span>
                      {row.problems > 0 ? (
                        <span className="wf-badge danger" title="The workflow has errors">
                          {row.problems}
                        </span>
                      ) : null}
                    </button>
                  </li>
                ))}
              </ul>
              {creating ? (
                <form
                  className="wf-new"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = event.currentTarget;
                    const name = (form.elements.namedItem('name') as HTMLInputElement).value.trim();
                    const preset = newPreset;
                    setCreating(false);
                    if (!name) return;
                    transport
                      .call('workflow.create', { name, ...(preset ? { preset } : {}) })
                      .then((made) => pick(made.workflow))
                      .catch(report);
                  }}
                >
                  <Input
                    name="name"
                    type="text"
                    placeholder="Name"
                    autoComplete="off"
                    className="h-7"
                    ref={focusWhenShown}
                    onKeyDown={(event) => {
                      event.stopPropagation();
                      if (event.key === 'Escape') setCreating(false);
                    }}
                  />
                  <Select
                    open={presetOpen}
                    onOpenChange={setPresetOpen}
                    value={newPreset || NONE}
                    onValueChange={(value) => setNewPreset(value === NONE ? '' : value)}
                  >
                    <SelectTrigger aria-label="Preset" size="sm" className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={NONE}>Only the trigger</SelectItem>
                      {(presets?.workflows ?? []).map((preset) => (
                        <SelectItem key={preset.id} value={preset.id}>
                          {preset.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <div className="wf-new-buttons">
                    <Button variant="ghost" size="xs" onClick={() => setCreating(false)}>
                      Cancel
                    </Button>
                    <Button type="submit" size="xs" className="ml-auto">
                      Create
                    </Button>
                  </div>
                </form>
              ) : (
                <div className="wf-side-actions">
                  <Button
                    variant="ghost"
                    size="xs"
                    className="px-1 text-muted-foreground"
                    onClick={() => {
                      setNewPreset('');
                      setCreating(true);
                    }}
                  >
                    + New
                  </Button>
                  {shown ? (
                    <>
                      <Button
                        variant="ghost"
                        size="xs"
                        className="px-1 text-muted-foreground"
                        onClick={() =>
                          transport
                            .call('workflow.duplicate', {
                              workflow: live.shown.get()?.workflow ?? '',
                            })
                            .then((copy) => pick(copy.workflow))
                            .catch(report)
                        }
                      >
                        Duplicate
                      </Button>
                      {confirmDelete ? (
                        <Button
                          variant="ghost"
                          size="xs"
                          className="px-1 text-destructive hover:text-destructive"
                          onClick={() => {
                            setConfirmDelete(false);
                            const name = live.shown.get()?.workflow;
                            if (!name) return;
                            transport
                              .call('workflow.delete', { workflow: name })
                              .then(() => {
                                live.shown.set(null);
                                live.selected.set(null);
                              })
                              .catch(report);
                          }}
                        >
                          Delete it
                        </Button>
                      ) : (
                        <Button
                          variant="ghost"
                          size="xs"
                          className="px-1 text-muted-foreground"
                          onClick={() => setConfirmDelete(true)}
                        >
                          Delete
                        </Button>
                      )}
                    </>
                  ) : null}
                </div>
              )}
              <h3 className="wf-side-title">
                Executions
                <Button
                  variant="ghost"
                  size="xs"
                  className="wf-side-toggle h-auto px-1 py-0 text-muted-foreground"
                  aria-pressed={allFolders}
                  onClick={() => setAllFolders((all) => !all)}
                >
                  {allFolders ? 'All folders' : 'This folder'}
                </Button>
              </h3>
              <ul className="wf-list">
                {rows.map((row) => (
                  <li key={row.id}>
                    <button
                      type="button"
                      className={cn('wf-list-item wf-execution', {
                        selected: row.id === execution,
                      })}
                      title={row.folder}
                      onClick={() => openExecution(row.id)}
                    >
                      <span className={`wf-dot tone-${tone(row.state)}`} aria-hidden="true" />
                      <span className="wf-list-name">{row.label}</span>
                      <span className="wf-muted">
                        {row.state === 'done' ? row.time_text : executionWord(row.state)}
                      </span>
                    </button>
                  </li>
                ))}
                {rows.length === 0 ? (
                  <li className="wf-muted wf-empty">
                    No execution{allFolders ? '' : ' in this folder'}.
                  </li>
                ) : null}
              </ul>
              {paused ? <p className="wf-warn">An execution is paused: a node failed.</p> : null}
            </div>
          </ScrollArea>
        </nav>
        <div className="wf-main">
          {openError ? (
            <p className="git-error git-banner" role="alert">
              {openError}
            </p>
          ) : null}
          {banner ? (
            <p className="git-note git-banner" role="status">
              {banner.text}
            </p>
          ) : null}
          {graph ? (
            <Canvas
              graph={graph}
              fitKey={execution ?? selected ?? ''}
              problems={execution ? [] : problems}
              execution={current}
              base={execution ? current?.base : facts?.base}
              selected={node}
              onSelect={setNode}
              onOpen={(id) => {
                if (!execution) setDetails(id);
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
          ) : (
            <p className="git-empty git-center">
              {workflowRows.length === 0
                ? 'No workflow yet. Click + New to make one, for example from the Review preset.'
                : 'Pick a workflow.'}
            </p>
          )}
        </div>
        {current && executionGraph ? (
          <ExecutionPanel execution={current} graph={executionGraph} node={node} />
        ) : null}
      </div>
      {picking ? (
        <NodePicker
          at={picking.screen}
          presets={presets?.nodes ?? []}
          onPick={addPreset}
          onClose={() => setPicking(null)}
        />
      ) : null}
      {details && detailsNode ? (
        <NodeDetails
          key={details}
          graph={shown?.graph ?? { name: '', nodes: [] }}
          node={detailsNode}
          problems={problems}
          choices={choices}
          last={last}
          onChange={(patch) => edit((g) => updateNode(g, details, patch))}
          onExecuteNode={() => {
            setDetails(null);
            execute(null, null, details);
          }}
          onClose={() => setDetails(null)}
        />
      ) : null}
      {executing && shown ? (
        <ExecuteDialog
          workflow={shown.workflow}
          name={shown.graph.name}
          folder={folder ?? ''}
          facts={facts}
          agents={agents}
          onExecute={(base, notes) => execute(base, notes)}
          onCancel={() => setExecuting(false)}
        />
      ) : null}
    </section>
  );
}
