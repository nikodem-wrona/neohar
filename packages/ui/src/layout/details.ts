// What the details of a pane show: the popover behind the button i of its title strip. The strip
// keeps what a person must see at a glance (the state, the harness, the title and the marks), and
// the details have the facts that a person reads when they ask: the folder, the cost, the tokens,
// the model, the version, the session and the size. The core gives each fact. This module only
// picks the rows and their words.

import type { AgentValue, PaneValue, Usage } from '@neohar/transport';
import { badgeOf, harnessName } from '../agents/badge';
import { costText, hasUsage } from '../timeline/model';

export interface DetailRow {
  /** The name of the row, for `data-detail`. */
  key: string;
  label: string;
  value: string;
  /** Data, in the font of data: a path, an id or a number. */
  data: boolean;
  /** The row has a button that copies the value. */
  copy: boolean;
}

/** The heading of the details: the harness of the run, else `Terminal`. */
export function detailsTitle(run: AgentValue | undefined): string {
  return run ? harnessName(run.harness) : 'Terminal';
}

/** The rows of the details of a pane, in the order they show. A fact that the core does not give has no row. */
export function paneDetails(pane: PaneValue, run: AgentValue | undefined): DetailRow[] {
  const rows: DetailRow[] = [];
  const add = (
    key: string,
    label: string,
    value: string | null | undefined,
    data: boolean,
    copy = false,
  ): void => {
    if (value) rows.push({ key, label, value, data, copy });
  };
  add('title', 'Title', stripTitle(pane.title, run?.harness), false);
  add('folder', 'Folder', pane.cwd, true, true);
  if (run) {
    add('model', 'Model', run.model, true);
    const version = run.harness_version;
    add(
      'version',
      'Version',
      version && badgeOf(run).unvalidated ? `${version} · not validated` : version,
      true,
    );
    add('cost', 'Cost', costText(run.usage), true);
    add('tokens', 'Tokens', hasUsage(run.usage) ? tokensText(run.usage) : null, true);
    add('session', 'Session', run.session_id, true, true);
  } else {
    add('program', 'Program', pane.fg_process, true);
  }
  add('pane', 'Pane', `${pane.handle} · ${pane.cols}×${pane.rows}`, true);
  return rows;
}

/** The tokens of a run: `1200 in · 340 out · 5000 cached`. */
export function tokensText(usage: Usage): string {
  const parts = [`${usage.input_tokens} in`, `${usage.output_tokens} out`];
  if (usage.cache_read_tokens > 0) parts.push(`${usage.cache_read_tokens} cached`);
  return parts.join(' · ');
}

/**
 * The title for the strip, beside the badge of the harness. It is empty when it only names the
 * harness, as `✳ Claude Code` does before Claude Code has a task, so the strip does not say the
 * name two times.
 */
export function stripTitle(title: string, harness: string | undefined): string {
  if (!harness) return title;
  const bare = title
    .replace(/^[^\p{L}\p{N}]+/u, '')
    .trim()
    .toLowerCase();
  return bare === harness.toLowerCase() || bare === harnessName(harness).toLowerCase() ? '' : title;
}
