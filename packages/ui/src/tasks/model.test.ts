import { expect, test } from 'bun:test';
import {
  RpcError,
  type FavoriteValue,
  type HarnessChoices,
  type HarnessValue,
  type TaskValue,
} from '@neohar/transport';
import {
  blockingHarness,
  createParams,
  defaultRepo,
  effortsOf,
  failed,
  favoriteOf,
  formOf,
  groupTasks,
  harnessProblem,
  isDraft,
  isOpen,
  keptEffort,
  menusOf,
  panelMissing,
  sizeLabel,
  statusLabel,
  taskOfTab,
  truncatedLabel,
  updateParams,
} from './model';

function task(partial: Partial<TaskValue>): TaskValue {
  return {
    id: 'k',
    handle: 'k1',
    title: 'Fix the login bug',
    repo: '/work/app',
    worktree: '/wt/app/fix',
    branch: 'neohar/fix',
    base: 'main',
    status: 'idle',
    ports: [41000],
    agents: [],
    attachments: [],
    created_at: '2026-10-11T00:00:00.000Z',
    ...partial,
  } as TaskValue;
}

test('a closed task is not open, and a draft is open', () => {
  expect(isOpen(task({ status: 'closed' }))).toBe(false);
  expect(isOpen(task({ status: 'running' }))).toBe(true);
  expect(isOpen(task({ status: 'draft' }))).toBe(true);
  expect(isDraft(task({ status: 'draft' }))).toBe(true);
  expect(isDraft(task({ status: 'idle' }))).toBe(false);
});

test('the rows fall into drafts, started and closed tasks, in the order of the core', () => {
  const rows = [
    task({ handle: 'k4', status: 'draft' }),
    task({ handle: 'k1', status: 'closed' }),
    task({ handle: 'k3', status: 'running' }),
    task({ handle: 'k2', status: 'draft' }),
    task({ handle: 'k6', status: 'setup_failed' }),
  ];
  const groups = groupTasks(rows);
  const handles = (list: TaskValue[]) => list.map((row) => row.handle);
  expect(handles(groups.drafts)).toEqual(['k4', 'k2']);
  expect(handles(groups.started)).toEqual(['k3', 'k6']);
  expect(handles(groups.closed)).toEqual(['k1']);
});

test('the task of a tab is the open one that has the tab', () => {
  const rows = [
    task({ handle: 'k1', tab: 'tab-a', status: 'closed' }),
    task({ handle: 'k2', tab: 'tab-a' }),
    task({ handle: 'k3', tab: 'tab-b' }),
  ];
  expect(taskOfTab(rows, 'tab-a')?.handle).toBe('k2');
  expect(taskOfTab(rows, 'tab-c')).toBeUndefined();
  expect(taskOfTab(rows, undefined)).toBeUndefined();
});

test('every status has a label, and only a failed setup is a failure', () => {
  expect(statusLabel('setup_failed')).toBe('Setup failed');
  expect(statusLabel('draft')).toBe('Draft');
  expect(statusLabel('running')).toBe('Running');
  expect(failed(task({ status: 'setup_failed' }))).toBe(true);
  expect(failed(task({ status: 'idle' }))).toBe(false);
});

test('the form needs a repository and a title', () => {
  expect(createParams({ repo: '', title: 'x', harness: '', prompt: '' })).toBeNull();
  expect(createParams({ repo: '/r', title: '  ', harness: '', prompt: '' })).toBeNull();
  expect(createParams({ repo: ' /r ', title: ' Fix it ', harness: '', prompt: '' })).toEqual({
    repo: '/r',
    title: 'Fix it',
  });
});

test('the prompt goes with or without a harness, and a blank prompt stays out', () => {
  expect(createParams({ repo: '/r', title: 't', harness: '', prompt: ' notes ' })).toEqual({
    repo: '/r',
    title: 't',
    prompt: 'notes',
  });
  expect(createParams({ repo: '/r', title: 't', harness: 'claude', prompt: ' go ' })).toEqual({
    repo: '/r',
    title: 't',
    harness: 'claude',
    prompt: 'go',
  });
  expect(createParams({ repo: '/r', title: 't', harness: 'codex', prompt: '  ' })).toEqual({
    repo: '/r',
    title: 't',
    harness: 'codex',
  });
});

test('a harness brings its model and effort level, and the defaults stay out', () => {
  const form = { repo: '/r', title: 't', prompt: '', model: 'opus[1m]', effort: 'high' };
  expect(createParams({ ...form, harness: 'claude' })).toEqual({
    repo: '/r',
    title: 't',
    harness: 'claude',
    model: 'opus[1m]',
    effort: 'high',
  });
  expect(createParams({ ...form, harness: 'claude', model: '', effort: '' })).toEqual({
    repo: '/r',
    title: 't',
    harness: 'claude',
  });
  // Without a harness, there is no agent to choose for.
  expect(createParams({ ...form, harness: '' })).toEqual({ repo: '/r', title: 't' });
});

test('picked and pasted files go as uploads', () => {
  const file = { key: 'p1', name: 'shot.png', bytes: 3, data: 'AAEC' };
  expect(createParams({ repo: '/r', title: 't', harness: '', prompt: '' }, [file])).toEqual({
    repo: '/r',
    title: 't',
    uploads: [{ name: 'shot.png', data: 'AAEC' }],
  });
});

test('a change of a draft sends what changed, and null for what the person emptied', () => {
  const draft = task({
    status: 'draft',
    harness: 'claude',
    model: 'opus',
    effort: 'high',
    prompt: 'go',
    attachments: [{ id: 'a1', name: 'plan.md', mime: 'text/markdown', bytes: 9 }],
  });
  const same = formOf(draft);
  expect(updateParams(draft, same)).toEqual({});
  expect(updateParams(draft, { ...same, title: ' ' })).toBeNull();
  expect(updateParams(draft, { ...same, title: ' Better ', prompt: '' })).toEqual({
    title: 'Better',
    prompt: null,
  });
  // Another agent starts from its defaults; no agent takes its choices away in the core.
  expect(updateParams(draft, { ...same, harness: 'codex', model: '', effort: '' })).toEqual({
    harness: 'codex',
    model: null,
    effort: null,
  });
  expect(updateParams(draft, { ...same, harness: '' })).toEqual({ harness: null });
  const file = { key: 'p1', name: 'shot.png', bytes: 3, data: 'AAEC' };
  expect(updateParams(draft, same, [file], ['a1', 'gone'])).toEqual({
    uploads: [{ name: 'shot.png', data: 'AAEC' }],
    remove: ['a1'],
  });
});

test('only an unsupported folder panel opens the folder browser', () => {
  expect(panelMissing(new RpcError('unsupported', 'cannot show a folder panel'))).toBe(true);
  expect(panelMissing(new RpcError('not_found', 'no window'))).toBe(false);
  expect(panelMissing(new Error('unsupported'))).toBe(false);
  expect(truncatedLabel(2000)).toContain('first 2000');
});

test('a size reads as bytes, KB or MB', () => {
  expect(sizeLabel(512)).toBe('512 B');
  expect(sizeLabel(3 * 1024)).toBe('3 KB');
  expect(sizeLabel(1.25 * 1024 * 1024)).toBe('1.3 MB');
});

test('a model has its own effort levels or those of the harness', () => {
  const all: HarnessChoices[] = [
    {
      harness: 'codex',
      models: [
        { id: 'gpt-a', label: 'GPT-A', efforts: ['low', 'max'] },
        { id: 'gpt-b', label: 'GPT-B' },
      ],
      efforts: ['low', 'high'],
      installed: true,
      login: 'logged_in',
      available: true,
    },
  ];
  const codex = menusOf(all, 'codex');
  expect(codex?.models.length).toBe(2);
  expect(menusOf(all, 'claude')).toBeUndefined();
  expect(menusOf(all, '')).toBeUndefined();
  expect(effortsOf(codex, 'gpt-a')).toEqual(['low', 'max']);
  expect(effortsOf(codex, 'gpt-b')).toEqual(['low', 'high']);
  expect(effortsOf(codex, '')).toEqual(['low', 'high']);
  expect(effortsOf(undefined, '')).toEqual([]);
  expect(keptEffort(['low', 'max'], 'low')).toBe('low');
  expect(keptEffort(['low', 'max'], 'high')).toBe('');
});

const favorites: FavoriteValue[] = [
  { path: '/work/api', name: 'api', used_at: '2026-10-14T10:00:02Z' },
  { path: '/work/app', name: 'app', used_at: '2026-10-14T10:00:01Z' },
];

test('a new task takes the favorite used last, else the root of the workspace', () => {
  expect(defaultRepo(favorites, '/work/web')).toBe('/work/api');
  expect(defaultRepo([], '/work/web')).toBe('/work/web');
  expect(defaultRepo([], '')).toBe('');
});

test('the field shows a favorite when its path is the path of one', () => {
  expect(favoriteOf(favorites, ' /work/app ')?.name).toBe('app');
  expect(favoriteOf(favorites, '/work/app/src')).toBeUndefined();
  expect(favoriteOf(favorites, '')).toBeUndefined();
});

function harnessRow(partial: Partial<HarnessValue>): HarnessValue {
  return {
    id: 'codex',
    name: 'Codex',
    installed: true,
    validated: false,
    login: 'logged_in',
    available: true,
    ...partial,
  };
}

test('a harness that the core marks as not available stops the start', () => {
  const rows = {
    claude: harnessRow({ id: 'claude', name: 'Claude Code', installed: false, available: false }),
    codex: harnessRow({ login: 'logged_out', available: false, hint: 'Run `codex login`.' }),
  };
  expect(blockingHarness(rows, 'codex')?.hint).toBe('Run `codex login`.');
  expect(harnessProblem(rows.codex)).toBe('Codex is not logged in.');
  expect(harnessProblem(rows.claude)).toBe('Claude Code is not installed.');
  // No agent, a harness that the core has not checked, and one with `unknown`: the start goes on.
  expect(blockingHarness(rows, '')).toBeUndefined();
  expect(blockingHarness(rows, null)).toBeUndefined();
  expect(blockingHarness({}, 'codex')).toBeUndefined();
  expect(blockingHarness({ codex: harnessRow({ login: 'unknown' }) }, 'codex')).toBeUndefined();
});
