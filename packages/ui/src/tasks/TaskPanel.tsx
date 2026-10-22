// The Tasks tab of the drawer: every task in three groups, the drafts, the tasks
// that started and the closed ones. A click on a row opens the task in the dialog, where the
// person changes it. A draft has a "Start" button: nothing starts before it. While a start runs,
// the row shows the step of its job, and a start that failed shows why. The Start button of a draft
// is off while its agent is not installed or not logged in. The core decides
// everything shown here.

import { RpcError, type TaskValue } from '@neohar/transport';
import { createSignal, For, Show } from 'solid-js';
import { useApp } from '../app/context';
import {
  blockingHarness,
  failed,
  groupTasks,
  harnessProblem,
  isDraft,
  statusLabel,
  taskOfTab,
} from './model';

/** Starts a draft and notes its job, so that the row can show the progress. */
export function useStart(): (task: TaskValue) => Promise<string | null> {
  const { transport, view } = useApp();
  return (task) =>
    transport
      .call('task.start', { task: task.id })
      .then((started) => {
        view.noteStart(task.id, started.job);
        return null;
      })
      .catch((failure: unknown) =>
        failure instanceof RpcError ? failure.message : 'The task did not start.',
      );
}

export function TaskPanel() {
  const { stores, windowId, view, report } = useApp();
  const groups = () => groupTasks(stores.tasks.rows());
  const tab = () => stores.windows.state.byKey[windowId]?.tab ?? undefined;
  const current = () => taskOfTab(stores.tasks.rows(), tab())?.id;
  const [showClosed, setShowClosed] = createSignal(false);
  const [refused, setRefused] = createSignal<Readonly<Record<string, string>>>({});
  const start = useStart();
  // A draft whose agent is not installed or not logged in waits.
  const blocking = (task: TaskValue) => blockingHarness(stores.harnesses.state.byKey, task.harness);
  const startTitle = (task: TaskValue): string => {
    const row = blocking(task);
    return row
      ? harnessProblem(row)
      : 'Make the worktree, the ports and the tab, and start the agent';
  };

  const job = (task: TaskValue) => {
    const id = view.startJobs()[task.id];
    return id === undefined ? undefined : stores.jobs.state.byKey[id];
  };
  // The line under a row: the step of a start that runs, or why the last start failed.
  const note = (task: TaskValue): { text: string; failed: boolean } | null => {
    const own = refused()[task.id];
    if (own) return { text: own, failed: true };
    const row = job(task);
    if (row?.status === 'running') return { text: row.message ?? 'Starting', failed: false };
    if (row?.status === 'failed') {
      return { text: row.error?.message ?? 'The start failed.', failed: true };
    }
    return null;
  };
  const startRow = (task: TaskValue): void => {
    setRefused((all) => Object.fromEntries(Object.entries(all).filter(([id]) => id !== task.id)));
    start(task)
      .then((message) => {
        if (message) setRefused((all) => ({ ...all, [task.id]: message }));
      })
      .catch(report);
  };

  const row = (task: TaskValue) => (
    <li class="task-row" classList={{ current: task.id === current() }}>
      <button
        type="button"
        class="task-open"
        classList={{ failed: failed(task) }}
        data-task={task.handle}
        title={`Open ${task.handle}`}
        onClick={() => view.setTaskDialog({ kind: 'task', task: task.id })}
      >
        <span class="handle">{task.handle}</span>
        <span class="task-row-title">{task.title}</span>
        <span class="task-row-status">{statusLabel(task.status)}</span>
      </button>
      <Show when={isDraft(task)}>
        <button
          type="button"
          class="task-start"
          aria-label={`Start ${task.handle}`}
          title={startTitle(task)}
          disabled={job(task)?.status === 'running' || blocking(task) !== undefined}
          onClick={() => startRow(task)}
        >
          Start
        </button>
      </Show>
      <Show when={note(task)}>
        {(line) => (
          <p
            class="task-row-note"
            classList={{ failed: line().failed }}
            role={line().failed ? 'alert' : 'status'}
          >
            {line().text}
          </p>
        )}
      </Show>
    </li>
  );

  return (
    <div class="task-panel">
      <button type="button" class="task-new" onClick={() => view.setTaskDialog({ kind: 'new' })}>
        New task
      </button>
      <Show
        when={stores.tasks.rows().length > 0}
        fallback={
          <p class="drawer-empty">No task yet. “New task” writes one, and it waits for “Start”.</p>
        }
      >
        <Show when={groups().drafts.length > 0}>
          <section class="task-group" aria-label="Drafts">
            <h3 class="task-group-title">Drafts</h3>
            <ul class="task-list">
              <For each={groups().drafts}>{row}</For>
            </ul>
          </section>
        </Show>
        <Show when={groups().started.length > 0}>
          <section class="task-group" aria-label="Started">
            <h3 class="task-group-title">Started</h3>
            <ul class="task-list">
              <For each={groups().started}>{row}</For>
            </ul>
          </section>
        </Show>
        <Show when={groups().closed.length > 0}>
          <section class="task-group" aria-label="Closed">
            <h3 class="task-group-title">
              <button
                type="button"
                class="task-group-toggle"
                aria-expanded={showClosed()}
                onClick={() => setShowClosed((shown) => !shown)}
              >
                Closed ({groups().closed.length})
              </button>
            </h3>
            <Show when={showClosed()}>
              <ul class="task-list">
                <For each={groups().closed}>{row}</For>
              </ul>
            </Show>
          </section>
        </Show>
      </Show>
    </div>
  );
}
