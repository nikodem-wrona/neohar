// What the badge of an agent shows. The core decides the state of a run. This
// module only chooses the glyph, the marks and the text for it: a `~` when the state is a guess,
// a `?` when the version of the harness is not validated, and a dot when the run is done (idle,
// and this window has not seen it).

import type { AgentValue, PaneValue, TaskValue } from '@neohar/transport';

export interface Badge {
  /** The glyph of the execution state. */
  glyph: string;
  /** The state in four letters for the strips of the board: `WORK`, `WAIT`, `IDLE`. */
  code: string;
  /** The run has an open interaction: an approval, a question or a plan waits for a person. */
  waiting: boolean;
  /** The state in words, for a screen reader. */
  state: string;
  /** The state comes from the screen or the process, not from the harness. */
  inferred: boolean;
  /** No recording of the golden corpus covers this version of the harness. */
  unvalidated: boolean;
  /** The run is idle and this window has not seen it since. */
  done: boolean;
  exited: boolean;
  /** The core found the harness in the process list and has no proof of the pane. */
  suggested: boolean;
  yolo: boolean;
  harness: string;
  /** The full text for the tooltip. */
  tooltip: string;
}

const GLYPHS: Record<string, string> = {
  starting: '◌',
  working: '●',
  idle: '○',
  error: '▲',
  rate_limited: '◷',
  exited: '×',
  unknown: '·',
};

const STATES: Record<string, string> = {
  starting: 'starting',
  working: 'working',
  idle: 'idle',
  error: 'error',
  rate_limited: 'rate limited',
  exited: 'ended',
  unknown: 'state unknown',
};

const CODES: Record<string, string> = {
  starting: 'INIT',
  working: 'WORK',
  idle: 'IDLE',
  error: 'ERR',
  rate_limited: 'RATE',
  exited: 'END',
  unknown: '?',
};

const HARNESSES: Record<string, string> = { claude: 'Claude Code', codex: 'Codex' };

export function harnessName(harness: string): string {
  return HARNESSES[harness] ?? harness;
}

/**
 * What a run works on, in the words that a person knows it by: the title of its task, else
 * the title of its pane, else the name of its harness. The board and the phone app show it.
 */
export function workOf(
  run: AgentValue,
  task: Pick<TaskValue, 'title'> | undefined,
  pane: Pick<PaneValue, 'title'> | undefined,
): string {
  return task?.title || pane?.title || harnessName(run.harness);
}

export function badgeOf(run: AgentValue): Badge {
  const execution = run.facets.execution;
  const name = execution.value.state;
  const exited = name === 'exited';
  const source = execution.provenance.source;
  const inferred = !exited && (source === 'screen' || source === 'process');
  const unvalidated = run.facets.adapter_health.value.state === 'unvalidated_version';
  const done = name === 'idle' && run.facets.read_state === 'unseen';
  const suggested = run.binding.state !== 'bound';
  const waiting = !exited && run.facets.pending.length > 0;
  const harness = harnessName(run.harness);
  const parts = [harness, done ? 'done' : (STATES[name] ?? name)];
  if (waiting) parts.push('waits for you');
  if (inferred) parts.push(source === 'screen' ? 'inferred from screen' : 'inferred from process');
  if (unvalidated) parts.push('version not validated');
  if (suggested) parts.push('not linked to its pane');
  if (run.yolo) parts.push('yolo');
  return {
    glyph: GLYPHS[name] ?? '·',
    code: waiting ? 'WAIT' : done ? 'DONE' : (CODES[name] ?? '?'),
    waiting,
    state: STATES[name] ?? name,
    inferred,
    unvalidated,
    done,
    exited,
    suggested,
    yolo: run.yolo === true,
    harness,
    tooltip: parts.join(' · '),
  };
}
