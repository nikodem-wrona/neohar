// What the Tasks tab and the task dialog need from the rows of `tasks`. The core decides the
// status, the ports, the lines of the diff and the files. This module only puts the rows in
// groups, turns a status into a label, and builds the params of `task.create` and `task.update`
// from a form. The menus of the model and the effort level come from `agent.choices`. The favorite
// folders come from `favorite.list`, the one used last first. Whether the agent of a task can
// start comes from the row of `harnesses`, which the core decides.

import {
  RpcError,
  type FavoriteValue,
  type Harness,
  type HarnessChoices,
  type HarnessValue,
  type TaskStatus,
  type TaskValue,
} from '@neohar/transport';

/** A task that is not closed. */
export function isOpen(task: TaskValue): boolean {
  return task.status !== 'closed';
}

/** A task that is kept and not started: everything of it can change. */
export function isDraft(task: TaskValue): boolean {
  return task.status === 'draft';
}

/** The task whose tab a window shows, if there is one. */
export function taskOfTab(
  tasks: readonly TaskValue[],
  tab: string | undefined,
): TaskValue | undefined {
  if (tab === undefined) return undefined;
  return tasks.find((task) => isOpen(task) && task.tab === tab);
}

const LABELS: Record<TaskStatus, string> = {
  draft: 'Draft',
  creating: 'Creating',
  setup: 'Setup',
  running: 'Running',
  idle: 'Idle',
  setup_failed: 'Setup failed',
  closed: 'Closed',
};

export function statusLabel(status: TaskStatus): string {
  return LABELS[status];
}

/** A task that needs a look: its setup failed. */
export function failed(task: TaskValue): boolean {
  return task.status === 'setup_failed';
}

export interface TaskGroups {
  /** Not started: the queue of the person. */
  drafts: TaskValue[];
  /** Started and not closed. */
  started: TaskValue[];
  closed: TaskValue[];
}

/** The rows of the Tasks tab, each group in the order of the projection, which the core sets. */
export function groupTasks(rows: readonly TaskValue[]): TaskGroups {
  return {
    drafts: rows.filter(isDraft),
    started: rows.filter((task) => isOpen(task) && !isDraft(task)),
    closed: rows.filter((task) => !isOpen(task)),
  };
}

export type HarnessChoice = '' | Harness;

export interface TaskForm {
  repo: string;
  title: string;
  harness: HarnessChoice;
  prompt: string;
  /** The model, or `''` for the one of the settings of the harness. */
  model?: string;
  /** The effort level, or `''` for the one of the settings of the harness. */
  effort?: string;
}

/** A file that the person picked or pasted, and that the core has not got yet. */
export interface PendingFile {
  /** A key for the list of the form. */
  key: string;
  name: string;
  bytes: number;
  /** The bytes as base64. */
  data: string;
}

/** The form of a task, as the dialog starts it. */
export function formOf(task: TaskValue): TaskForm {
  return {
    repo: task.repo,
    title: task.title,
    harness: task.harness ?? '',
    prompt: task.prompt ?? '',
    model: task.model ?? '',
    effort: task.effort ?? '',
  };
}

function uploadsOf(files: readonly PendingFile[]): { name: string; data: string }[] {
  return files.map((file) => ({ name: file.name, data: file.data }));
}

/**
 * The params of `task.create`, or `null` while the form lacks the repository or the title. The
 * task is a draft: nothing starts until `task.start`.
 */
export function createParams(
  form: TaskForm,
  files: readonly PendingFile[] = [],
): Record<string, unknown> | null {
  const repo = form.repo.trim();
  const title = form.title.trim();
  if (repo === '' || title === '') return null;
  const params: Record<string, unknown> = { repo, title };
  const prompt = form.prompt.trim();
  if (prompt !== '') params.prompt = prompt;
  if (form.harness !== '') {
    params.harness = form.harness;
    if (form.model) params.model = form.model;
    if (form.effort) params.effort = form.effort;
  }
  if (files.length > 0) params.uploads = uploadsOf(files);
  return params;
}

/**
 * The params of `task.update` for a draft: the fields that the form changed, with `null` for a
 * field that the person emptied. `{}` when nothing changed, and `null` while the form lacks the
 * repository or the title.
 */
export function updateParams(
  task: TaskValue,
  form: TaskForm,
  files: readonly PendingFile[] = [],
  removed: readonly string[] = [],
): Record<string, unknown> | null {
  const repo = form.repo.trim();
  const title = form.title.trim();
  if (repo === '' || title === '') return null;
  const before = formOf(task);
  const params: Record<string, unknown> = {};
  if (title !== before.title) params.title = title;
  if (repo !== before.repo) params.repo = repo;
  const text = (value: string | undefined): string | null => {
    const trimmed = (value ?? '').trim();
    return trimmed === '' ? null : trimmed;
  };
  const prompt = text(form.prompt);
  if (prompt !== text(before.prompt)) params.prompt = prompt;
  const harness = form.harness === '' ? null : form.harness;
  if (harness !== (task.harness ?? null)) params.harness = harness;
  // Without an agent, the core drops the model and the effort level itself.
  const model = harness ? text(form.model) : null;
  const effort = harness ? text(form.effort) : null;
  if (harness && model !== (task.model ?? null)) params.model = model;
  if (harness && effort !== (task.effort ?? null)) params.effort = effort;
  if (files.length > 0) params.uploads = uploadsOf(files);
  const gone = removed.filter((id) => task.attachments.some((file) => file.id === id));
  if (gone.length > 0) params.remove = gone;
  return params;
}

/** A size for a person: bytes, KB or MB. */
export function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/**
 * Whether a failure of `window.pick_folder` means that the host has no folder panel. Then the
 * dialog shows the folder browser (`fs.dirs`). Any other failure is an error to show.
 */
export function panelMissing(failure: unknown): boolean {
  return failure instanceof RpcError && failure.code === 'unsupported';
}

/** The line under a folder list that the core cut (`fs.dirs` sends at most 2,000 folders). */
export function truncatedLabel(shown: number): string {
  return `This folder has more folders. The list shows the first ${shown}. Type the path.`;
}

/**
 * The row of `harnesses` that stops the start of a task with this agent: the harness is not
 * installed, or it is not logged in. `undefined` when the start can go on, also while the core has
 * not checked the harness yet.
 */
export function blockingHarness(
  harnesses: Readonly<Record<string, HarnessValue>>,
  harness: HarnessChoice | null | undefined,
): HarnessValue | undefined {
  if (!harness) return undefined;
  const row = harnesses[harness];
  return row && !row.available ? row : undefined;
}

/** What is wrong with a harness that stops a start. */
export function harnessProblem(row: HarnessValue): string {
  return row.installed ? `${row.name} is not logged in.` : `${row.name} is not installed.`;
}

/** The menus of the chosen harness, or none when no harness is chosen. */
export function menusOf(
  all: readonly HarnessChoices[],
  harness: HarnessChoice,
): HarnessChoices | undefined {
  return harness === '' ? undefined : all.find((menus) => menus.harness === harness);
}

/** The effort levels of a model: its own, or those of the harness. `''` is the default model. */
export function effortsOf(menus: HarnessChoices | undefined, model: string): readonly string[] {
  if (!menus) return [];
  const own = menus.models.find((choice) => choice.id === model)?.efforts;
  return own && own.length > 0 ? own : menus.efforts;
}

/** The effort level after a change of the model: the same one when the levels have it. */
export function keptEffort(efforts: readonly string[], effort: string): string {
  return efforts.includes(effort) ? effort : '';
}

/**
 * The repository of a new task: the favorite folder used last, which the core gives first, else
 * the root of the workspace of the window.
 */
export function defaultRepo(favorites: readonly FavoriteValue[], root: string): string {
  return favorites[0]?.path ?? root;
}

/** The favorite folder that the field of the repository shows, if it shows one. */
export function favoriteOf(
  favorites: readonly FavoriteValue[],
  repo: string,
): FavoriteValue | undefined {
  const path = repo.trim();
  return favorites.find((favorite) => favorite.path === path);
}
