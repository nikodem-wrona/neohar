// The dialog of a task. It floats over the window and never covers it for good:
// Escape, "Cancel" or a click beside the box closes it. It is a modal `<dialog>`, so the
// rest of the window is inert while it is open.
//
// For a new task and for a draft it is the form: the repository, the title, the prompt, the files,
// and the agent with its model and effort level. "Create task" keeps a draft; nothing runs until
// "Start". A draft can also be saved and deleted. "Add files…" and a paste of files, such as a
// screenshot on the clipboard, add files to the form. For a task that started the dialog shows
// its facts and the buttons that close it, and only its title can change.
//
// "Choose…" asks the core for the folder panel of the system (`window.pick_folder`). When
// the host has no panel (`unsupported`), the form shows a folder browser instead, which lists the
// folders of the machine of the window with `fs.dirs`. The menus of the model and the effort level come from `agent.choices`; "Default" leaves the
// choice to the settings of the harness.
//
// The star beside the repository adds the folder to the favorites or removes it, and a click on a
// favorite under the field puts it in the field. A new task starts with the favorite that the
// person used last, which the projection `favorites` has first.
//
// An agent that is not installed or not logged in on this machine shows why under the menus, with
// the hint of the core. The form keeps a draft with it, and Start is off.

import { RpcError, type HarnessChoices, type TaskValue } from '@neohar/transport';
import {
  createSignal,
  createUniqueId,
  For,
  Match,
  onCleanup,
  onMount,
  Show,
  Switch,
  untrack,
  type JSX,
} from 'solid-js';
import { useApp } from '../app/context';
import { outside } from '../shell/modal';
import type { TaskDialog as Shown } from '../app/view';
import { filesOfPaste, pendingFiles } from './files';
import {
  blockingHarness,
  createParams,
  defaultRepo,
  effortsOf,
  failed,
  favoriteOf,
  formOf,
  harnessProblem,
  isDraft,
  keptEffort,
  menusOf,
  panelMissing,
  sizeLabel,
  statusLabel,
  updateParams,
  type HarnessChoice,
  type PendingFile,
  type TaskForm,
} from './model';
import { FolderBrowser } from './FolderBrowser';
import { useStart } from './TaskPanel';

function messageOf(failure: unknown, fallback: string): string {
  return failure instanceof RpcError ? failure.message : fallback;
}

export function TaskDialog(props: { dialog: Shown }) {
  const { stores, view } = useApp();
  const titleId = createUniqueId();
  const task = (): TaskValue | undefined =>
    props.dialog.kind === 'task' ? stores.tasks.state.byKey[props.dialog.task] : undefined;
  const draft = () => {
    const row = task();
    return row && isDraft(row) ? row : undefined;
  };
  const close = (): void => {
    view.setTaskDialog(null);
  };
  // A click beside the box keeps a form that has changes: the text of a prompt is work.
  let changed: () => boolean = () => false;
  let dialog: HTMLDialogElement | undefined;
  onMount(() => dialog?.showModal());
  onCleanup(() => {
    if (dialog?.open) dialog.close();
  });
  return (
    // WebKit gives no focus to a button that the person clicks. The box takes it, so that Escape
    // still reaches it after a click on a favorite or a star.
    <dialog
      class="task-dialog"
      aria-labelledby={titleId}
      tabIndex={-1}
      ref={(element) => (dialog = element)}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && outside(event, event.currentTarget)) {
          if (!changed()) close();
        }
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopPropagation();
          close();
        }
      }}
    >
      <Switch
        fallback={
          <div class="task-form">
            <h2 id={titleId} class="task-dialog-title">
              No such task
            </h2>
            <p class="task-dialog-text">The task is gone.</p>
            <div class="task-dialog-buttons">
              <button type="button" onClick={close}>
                Close
              </button>
            </div>
          </div>
        }
      >
        <Match when={props.dialog.kind === 'new'}>
          <DraftForm titleId={titleId} onClose={close} changes={(f) => (changed = f)} />
        </Match>
        <Match when={draft()}>
          {(row) => (
            <DraftForm
              task={row()}
              titleId={titleId}
              onClose={close}
              changes={(f) => (changed = f)}
            />
          )}
        </Match>
        <Match when={task()}>
          {(row) => <StartedTask task={row()} titleId={titleId} onClose={close} />}
        </Match>
      </Switch>
    </dialog>
  );
}

/** The form of a new task, or of a draft when `task` is given. */
function DraftForm(props: {
  task?: TaskValue;
  titleId: string;
  onClose: () => void;
  changes: (changed: () => boolean) => void;
}) {
  const { stores, transport, windowId, view, platform } = useApp();
  const startTask = useStart();
  // The repository of a new task starts as the root of the workspace that the window shows.
  const root = () => {
    const id = stores.windows.state.byKey[windowId]?.workspace;
    return (id ? stores.tree.state.byKey[id]?.root : null) ?? '';
  };
  const first = untrack(() => (props.task ? formOf(props.task) : undefined));
  const [repo, setRepo] = createSignal<string | null>(first?.repo ?? null);
  const [title, setTitle] = createSignal(first?.title ?? '');
  const [prompt, setPrompt] = createSignal(first?.prompt ?? '');
  const [harness, setHarness] = createSignal<HarnessChoice>(first?.harness ?? '');
  const [model, setModel] = createSignal(first?.model ?? '');
  const [effort, setEffort] = createSignal(first?.effort ?? '');
  const [choices, setChoices] = createSignal<HarnessChoices[]>([]);
  const favorites = () => stores.favorites.rows();
  const [starring, setStarring] = createSignal(false);
  const [pending, setPending] = createSignal<PendingFile[]>([]);
  const [removed, setRemoved] = createSignal<string[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [picking, setPicking] = createSignal(false);
  // The folder browser, when the host has no folder panel. A new object opens it again.
  const [browser, setBrowser] = createSignal<{ start: string } | null>(null);
  const [deleting, setDeleting] = createSignal(false);
  const ids = {
    repo: createUniqueId(),
    title: createUniqueId(),
    prompt: createUniqueId(),
    favorites: createUniqueId(),
  };
  let repoInput: HTMLInputElement | undefined;
  let titleInput: HTMLInputElement | undefined;
  let fileInput: HTMLInputElement | undefined;

  // Until the person changes the field, a new task shows the favorite used last.
  const repoValue = () => repo() ?? defaultRepo(favorites(), root());
  const favorite = () => favoriteOf(favorites(), repoValue());
  const form = (): TaskForm => ({
    repo: repoValue(),
    title: title(),
    harness: harness(),
    prompt: prompt(),
    model: model(),
    effort: effort(),
  });
  const changes = () =>
    props.task ? updateParams(props.task, form(), pending(), removed()) : null;
  const ready = () => (props.task ? changes() !== null : createParams(form(), pending()) !== null);
  props.changes(() =>
    props.task
      ? Object.keys(changes() ?? { title: '' }).length > 0
      : title().trim() !== '' || prompt().trim() !== '' || pending().length > 0,
  );

  onMount(() => {
    titleInput?.focus();
    // Without the menus, the start uses the settings of the harness.
    transport
      .call('agent.choices', {})
      .then((result) => setChoices(result.harnesses))
      .catch(() => setChoices([]));
  });
  const menus = () => menusOf(choices(), harness());
  const blocked = () => blockingHarness(stores.harnesses.state.byKey, harness());
  const efforts = () => effortsOf(menus(), model());
  const chooseHarness = (next: HarnessChoice): void => {
    setHarness(next);
    setModel('');
    setEffort('');
  };
  const chooseModel = (next: string): void => {
    setModel(next);
    setEffort(keptEffort(effortsOf(menus(), next), effort()));
  };

  const showRepo = (path: string): void => {
    setRepo(path);
    // A long path shows its end, the name of the folder.
    if (repoInput) repoInput.scrollLeft = repoInput.scrollWidth;
  };
  const star = (): void => {
    const path = repoValue().trim();
    if (path === '' || starring()) return;
    setStarring(true);
    setError(null);
    // The field keeps its path. Else a new task would show the next favorite after a removal.
    setRepo(path);
    const shown = favorite();
    const changed = shown
      ? transport.call('favorite.remove', { path: shown.path }).then(() => undefined)
      : transport.call('favorite.add', { path }).then((result) => showRepo(result.path));
    changed
      .catch((failure: unknown) => setError(messageOf(failure, 'The favorites did not change.')))
      .finally(() => setStarring(false));
  };

  // The focus goes back to the field, so that Escape still reaches the dialog.
  const closeBrowser = (): void => {
    setBrowser(null);
    repoInput?.focus();
  };

  const choose = (): void => {
    setPicking(true);
    setError(null);
    const start = repoValue().trim();
    transport
      .call('window.pick_folder', {
        window: windowId,
        title: 'Choose the repository',
        ...(start ? { start } : {}),
      })
      .then((picked) => {
        if (picked.path) showRepo(picked.path);
      })
      .catch((failure: unknown) => {
        // No panel on this host: the page browses the folders of the core.
        if (panelMissing(failure)) setBrowser({ start });
        else setError(messageOf(failure, 'The folder panel did not open.'));
      })
      .finally(() => setPicking(false));
  };

  const add = (files: Iterable<File>): void => {
    void pendingFiles(files)
      .then((read) => setPending((all) => [...all, ...read]))
      .catch(() => setError('A file did not read.'));
  };
  const onPaste = (event: ClipboardEvent): void => {
    const files = filesOfPaste(event);
    if (files.length === 0) return;
    event.preventDefault();
    add(files);
  };
  const kept = () => (props.task?.attachments ?? []).filter((file) => !removed().includes(file.id));

  /** Stores the form: a new draft, or the changes of a draft. Gives the id of the task. */
  const save = async (): Promise<string | null> => {
    setError(null);
    if (props.task) {
      const body = changes();
      if (body === null) return null;
      if (Object.keys(body).length > 0) {
        await transport.call('task.update', { task: props.task.id, ...body } as never);
      }
      return props.task.id;
    }
    const body = createParams(form(), pending());
    if (body === null) return null;
    const created = await transport.call('task.create', body as never);
    return created.task;
  };
  const run = (work: () => Promise<void>): void => {
    if (busy()) return;
    setBusy(true);
    work()
      .catch((failure: unknown) => setError(messageOf(failure, 'The task did not change.')))
      .finally(() => setBusy(false));
  };
  const submit = (event: Event): void => {
    event.preventDefault();
    if (!ready()) return;
    run(async () => {
      const id = await save();
      if (id === null) return;
      if (!props.task) view.showDrawer('tasks');
      props.onClose();
    });
  };
  const start = (): void => {
    const task = props.task;
    if (!task || !ready()) return;
    run(async () => {
      await save();
      const refused = await startTask(task);
      if (refused) {
        setError(refused);
        return;
      }
      props.onClose();
    });
  };
  const remove = (): void => {
    const task = props.task;
    if (!task) return;
    if (!deleting()) {
      setDeleting(true);
      return;
    }
    run(async () => {
      await transport.call('task.delete', { task: task.id });
      props.onClose();
    });
  };
  const paste = platform === 'macos' ? '⌘V' : 'Ctrl+V';

  return (
    <form
      class="task-form"
      aria-label={props.task ? `Draft ${props.task.handle}` : 'Create a task'}
      onSubmit={submit}
      onPaste={onPaste}
      onKeyDown={(event) => {
        // ⌘Enter or Ctrl+Enter keeps the form from any field, also from the prompt.
        if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) submit(event);
      }}
    >
      <h2 id={props.titleId} class="task-dialog-title">
        <Show when={props.task} fallback="New task">
          {(task) => (
            <>
              <span class="handle">{task().handle}</span> Draft
            </>
          )}
        </Show>
      </h2>
      <div class="task-field">
        <label for={ids.repo}>Repository</label>
        <div class="task-path">
          <input
            id={ids.repo}
            ref={(element) => (repoInput = element)}
            type="text"
            value={repoValue()}
            title={repoValue()}
            onInput={(event) => setRepo(event.currentTarget.value)}
            placeholder="/path/to/repository"
          />
          <button
            type="button"
            class="task-star"
            aria-label="Favorite folder"
            aria-pressed={favorite() !== undefined}
            onClick={star}
            // A disabled button loses the focus, and Escape would not reach the dialog: a click
            // while a change runs does nothing instead.
            aria-busy={starring()}
            disabled={repoValue().trim() === ''}
            title={
              favorite()
                ? 'Remove the folder from the favorites'
                : 'Add the folder to the favorites'
            }
          >
            {favorite() ? '★' : '☆'}
          </button>
          <button
            type="button"
            onClick={choose}
            disabled={picking()}
            title="Choose the repository in the Finder"
          >
            Choose…
          </button>
        </div>
        <Show when={browser()} keyed>
          {(open) => (
            <FolderBrowser
              start={open.start}
              onChoose={(path) => {
                showRepo(path);
                closeBrowser();
              }}
              onClose={closeBrowser}
            />
          )}
        </Show>
        <Show when={favorites().length > 0}>
          <div class="task-favorites" role="group" aria-labelledby={ids.favorites}>
            <span class="task-hint" id={ids.favorites}>
              Favorites
            </span>
            <For each={favorites()}>
              {(folder) => (
                <button
                  type="button"
                  class="task-favorite"
                  aria-pressed={folder.path === favorite()?.path}
                  title={folder.path}
                  onClick={() => showRepo(folder.path)}
                >
                  {folder.name}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
      <div class="task-field">
        <label for={ids.title}>Title</label>
        <input
          id={ids.title}
          ref={(element) => (titleInput = element)}
          type="text"
          value={title()}
          onInput={(event) => setTitle(event.currentTarget.value)}
          placeholder="Fix the login bug"
        />
      </div>
      <div class="task-field">
        <label for={ids.prompt}>Prompt</label>
        <textarea
          id={ids.prompt}
          rows="7"
          value={prompt()}
          onInput={(event) => setPrompt(event.currentTarget.value)}
          placeholder="What the agent does. It gets this text when the task starts."
        />
      </div>
      <div class="task-field">
        <span class="task-label" id={`${ids.prompt}-files`}>
          Files
        </span>
        <ul class="task-files" aria-labelledby={`${ids.prompt}-files`}>
          <For each={kept()}>
            {(file) => (
              <FileRow
                name={file.name}
                bytes={file.bytes}
                onRemove={() => setRemoved((all) => [...all, file.id])}
              />
            )}
          </For>
          <For each={pending()}>
            {(file) => (
              <FileRow
                name={file.name}
                bytes={file.bytes}
                onRemove={() => setPending((all) => all.filter((f) => f.key !== file.key))}
              />
            )}
          </For>
        </ul>
        <div class="task-files-add">
          <button type="button" onClick={() => fileInput?.click()}>
            Add files…
          </button>
          <span class="task-hint">or paste a screenshot with {paste}</span>
          <input
            ref={(element) => (fileInput = element)}
            type="file"
            multiple
            hidden
            aria-label="Files of the task"
            onChange={(event) => {
              const files = Array.from(event.currentTarget.files ?? []);
              event.currentTarget.value = '';
              if (files.length > 0) add(files);
            }}
          />
        </div>
      </div>
      <div class="task-pair">
        <label class="task-agent">
          Agent
          <select
            value={harness()}
            onChange={(event) => chooseHarness(event.currentTarget.value as HarnessChoice)}
          >
            <option value="">None</option>
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
          </select>
        </label>
        <Show when={(menus()?.models.length ?? 0) > 0}>
          <label class="task-model">
            Model
            <select value={model()} onChange={(event) => chooseModel(event.currentTarget.value)}>
              <option value="">Default</option>
              <For each={menus()?.models ?? []}>
                {(choice) => <option value={choice.id}>{choice.label}</option>}
              </For>
            </select>
          </label>
        </Show>
        <Show when={efforts().length > 0}>
          <label class="task-effort">
            Effort
            <select value={effort()} onChange={(event) => setEffort(event.currentTarget.value)}>
              <option value="">Default</option>
              <For each={efforts()}>{(level) => <option value={level}>{level}</option>}</For>
            </select>
          </label>
        </Show>
      </div>
      <Show when={blocked()}>
        {(row) => (
          <p class="task-harness" role="status">
            {harnessProblem(row())}
            <Show when={row().hint}>{(hint) => <span class="task-hint"> {hint()}</span>}</Show>
          </p>
        )}
      </Show>
      <Show when={error()}>
        <p class="task-error" role="alert">
          {error()}
        </p>
      </Show>
      <div class="task-dialog-buttons">
        <Show
          when={props.task}
          fallback={
            <button type="submit" class="primary" disabled={!ready() || busy()}>
              Create task
            </button>
          }
        >
          <button
            type="button"
            class="primary"
            disabled={!ready() || busy() || blocked() !== undefined}
            onClick={start}
            title={
              blocked()
                ? 'The agent of the task cannot start on this machine'
                : 'Make the worktree, the ports and the tab, and start the agent'
            }
          >
            Start
          </button>
          <button type="submit" disabled={!ready() || busy()}>
            Save
          </button>
          <button
            type="button"
            class="danger"
            disabled={busy()}
            onClick={remove}
            onBlur={() => setDeleting(false)}
          >
            {deleting() ? 'Delete for good' : 'Delete'}
          </button>
        </Show>
        <button type="button" class="task-cancel" onClick={props.onClose}>
          Cancel
        </button>
      </div>
    </form>
  );
}

function FileRow(props: {
  name: string;
  bytes: number;
  path?: string | undefined;
  onRemove?: () => void;
}) {
  return (
    <li class="task-file">
      <span class="task-file-name" title={props.path ?? props.name}>
        {props.name}
      </span>
      <span class="task-file-size">{sizeLabel(props.bytes)}</span>
      <Show when={props.onRemove}>
        {(onRemove) => (
          <button
            type="button"
            class="task-file-remove"
            aria-label={`Remove ${props.name}`}
            onClick={() => onRemove()()}
          >
            ×
          </button>
        )}
      </Show>
    </li>
  );
}

/** A task that started: its facts, its title, and the buttons that show its tab or close it. */
function StartedTask(props: { task: TaskValue; titleId: string; onClose: () => void }) {
  const { transport, windowId } = useApp();
  const [title, setTitle] = createSignal(untrack(() => props.task.title));
  const [message, setMessage] = createSignal<string | null>(null);
  const [needsForce, setNeedsForce] = createSignal(false);
  const titleField = createUniqueId();
  const closed = () => props.task.status === 'closed';
  const renamed = () => title().trim() !== '' && title().trim() !== props.task.title;

  const rename = (event: Event): void => {
    event.preventDefault();
    if (!renamed()) return;
    setMessage(null);
    transport
      .call('task.update', { task: props.task.id, title: title().trim() })
      .catch((failure: unknown) => setMessage(messageOf(failure, 'The title did not change.')));
  };
  const close = (remove: boolean, force: boolean): void => {
    setMessage(null);
    transport
      .call('task.close', { task: props.task.id, remove_worktree: remove, force })
      .then(() => setNeedsForce(false))
      .catch((failure: unknown) => {
        if (failure instanceof RpcError && failure.code === 'confirm_required') setNeedsForce(true);
        setMessage(messageOf(failure, 'The task did not close.'));
      });
  };
  const showTab = (): void => {
    const tab = props.task.tab;
    if (!tab) return;
    transport
      .call('tab.select', { tab, window: windowId })
      .then(props.onClose)
      .catch((failure: unknown) => setMessage(messageOf(failure, 'The tab did not show.')));
  };
  const remove = (): void => {
    transport
      .call('task.delete', { task: props.task.id })
      .then(props.onClose)
      .catch((failure: unknown) => setMessage(messageOf(failure, 'The task did not go.')));
  };
  const fact = (term: string, value: JSX.Element) => (
    <>
      <dt>{term}</dt>
      <dd>{value}</dd>
    </>
  );

  return (
    <form class="task-form" aria-label={`Task ${props.task.handle}`} onSubmit={rename}>
      <h2 id={props.titleId} class="task-dialog-title">
        <span class="handle">{props.task.handle}</span> {props.task.title}
      </h2>
      <div class="task-field">
        <label for={titleField}>Title</label>
        <div class="task-path">
          <input
            id={titleField}
            type="text"
            value={title()}
            onInput={(event) => setTitle(event.currentTarget.value)}
          />
          <button type="submit" disabled={!renamed()}>
            Save
          </button>
        </div>
      </div>
      <dl class="task-facts">
        <dt>Status</dt>
        <dd class="task-status" classList={{ failed: failed(props.task) }}>
          {statusLabel(props.task.status)}
        </dd>
        {fact('Repository', <span class="path">{props.task.repo}</span>)}
        <Show when={props.task.branch}>{(branch) => fact('Branch', branch())}</Show>
        <Show when={props.task.base}>{(base) => fact('Base', base())}</Show>
        <Show when={props.task.worktree}>
          {(worktree) => fact('Worktree', <span class="path">{worktree()}</span>)}
        </Show>
        {fact(
          'Ports',
          <For each={props.task.ports} fallback={<span>none</span>}>
            {(port) => <span class="port">{port}</span>}
          </For>,
        )}
        <Show when={props.task.harness}>
          {(harness) =>
            fact(
              'Agent',
              [
                harness() === 'claude' ? 'Claude Code' : 'Codex',
                props.task.model,
                props.task.effort,
              ]
                .filter(Boolean)
                .join(' · '),
            )
          }
        </Show>
        {fact('Agents', String(props.task.agents.length))}
        <Show when={props.task.diff_stats}>
          {(stats) =>
            fact(
              'Changes',
              <span class="diff-stats">
                <span class="add">+{stats().add}</span> <span class="del">−{stats().del}</span>
              </span>,
            )
          }
        </Show>
      </dl>
      <Show when={props.task.prompt}>
        {(text) => (
          <div class="task-field">
            <span class="task-label">Prompt</span>
            <p class="task-prompt">{text()}</p>
          </div>
        )}
      </Show>
      <Show when={props.task.attachments.length > 0}>
        <div class="task-field">
          <span class="task-label">Files</span>
          <ul class="task-files">
            <For each={props.task.attachments}>
              {(file) => (
                <FileRow name={file.name} bytes={file.bytes} path={file.path ?? undefined} />
              )}
            </For>
          </ul>
        </div>
      </Show>
      <Show when={message()}>
        <p class="task-error" role="alert">
          {message()}
        </p>
      </Show>
      <div class="task-dialog-buttons">
        <Show when={props.task.tab && !closed()}>
          <button type="button" class="primary" onClick={showTab}>
            Show tab
          </button>
        </Show>
        <Show
          when={!closed()}
          fallback={
            <button type="button" class="danger" onClick={remove}>
              Delete
            </button>
          }
        >
          <button type="button" onClick={() => close(false, false)}>
            Close task
          </button>
          <Show
            when={needsForce()}
            fallback={
              <button type="button" onClick={() => close(true, false)}>
                Close and remove worktree
              </button>
            }
          >
            <button type="button" class="danger" onClick={() => close(true, true)}>
              Remove anyway
            </button>
          </Show>
        </Show>
        <button type="button" class="task-cancel" onClick={props.onClose}>
          Done
        </button>
      </div>
    </form>
  );
}
