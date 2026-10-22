// Conventional Commits for neohar: the types, the scopes, the parser and the linter.
//
// The commit-msg hook (.githooks/commit-msg), `mise run check:commits`, the commits workflow and
// `mise run release` all read commits through this file, so they agree on one grammar:
//
//   <type>[(<scope>)][!]: <description>
//
//   [body]
//
//   [BREAKING CHANGE: <what breaks, and what a user must do>]
//
// TYPES and SCOPES are the only permitted values. A change that needs a new scope adds its row.
//
// Usage:
//   bun scripts/commits.ts --file <path>                 the commit-msg hook
//   bun scripts/commits.ts --range <from>..<to>          every commit of a range, no merges
//   bun scripts/commits.ts --title <text> [--range ...]  the title of a pull request
//   bun scripts/commits.ts --unpushed                    the commits that @{upstream} does not have

export const TYPES: Record<string, string> = {
  feat: 'a new feature for users of neohar',
  fix: 'a bug fix for users of neohar',
  perf: 'a change that makes neohar faster or smaller, with the numbers in the body',
  refactor: 'a change of the code that changes no behavior',
  docs: 'documentation only: the guide, README.md, CONTRIBUTING.md, comments',
  test: 'tests only',
  build: 'the build, the toolchain, the dependencies, mise.toml',
  ci: 'the GitHub workflows',
  style: 'formatting only',
  chore: 'other work that changes no shipped file',
  revert: 'undoes an earlier commit',
};

export const SCOPES: Record<string, string> = {
  agents: 'crates/nh-agents: the adapters of Claude Code and Codex, hooks and screen rules',
  api: 'crates/nh-proto: the JSON-RPC API, the wire types and generated.ts',
  cli: 'crates/nh: the nh binary, its commands, the hook shim and nh daemon',
  core: 'crates/nh-core: the state actor, projections, interactions and policy',
  deps: 'a change of the dependencies only',
  desktop: 'apps/desktop: the Tauri app, its windows and native integrations',
  golden: 'tools/nh-golden, tools/nh-record and tests/harness-golden: the recordings',
  holder: 'crates/nh-hold: the PTY owner of a pane',
  phone: 'apps/web and crates/nh-web: the phone app and its listener',
  redact: 'crates/nh-redact: the redaction of secrets',
  release: 'only `chore(release): <version>`, which `mise run release` makes',
  remote: 'crates/nh-remote: the links to other machines over ssh',
  rich: 'crates/nh-rich: the rich content of the cards and the plan view',
  store: 'crates/nh-store: the SQLite database and its schema',
  tasks: 'crates/nh-tasks: task worktrees, ports and setup commands',
  term: 'crates/nh-term: the VT engine, frames and snapshots',
  tools: 'tools/ and scripts/: the test, bench and check tools',
  ui: 'packages/: the webview, the renderer and the transport',
  website: 'website/: the documentation site',
};

/** The longest header, without the ` (#123)` that GitHub adds to a squash merge. */
export const HEADER_LIMIT = 72;

export type Level = 'major' | 'minor' | 'patch';

export type Commit = {
  type: string;
  scope: string | null;
  breaking: boolean;
  description: string;
  /** The text of the `BREAKING CHANGE:` footer, or null. */
  breakingNote: string | null;
  /** The number of the pull request in a ` (#123)` suffix, or null. */
  pr: number | null;
};

const HEADER = /^(?<type>[a-z]+)(?:\((?<scope>[^()]*)\))?(?<bang>!)?: (?<description>\S.*)$/;
const PR_SUFFIX = / \(#(\d+)\)$/;
const FOOTER = /^BREAKING[ -]CHANGE: /;
const FIXUP = /^(fixup|squash|amend)! /;
const SEMVER = /^\d+\.\d+\.\d+$/;
const DOC = 'See "Commit messages" in CONTRIBUTING.md.';

/**
 * The message as git stores it: no comment lines, nothing after the scissors line of
 * `git commit -v`, no trailing blank lines. The commit-msg hook gets the raw file.
 */
export function cleanMessage(raw: string): string {
  const lines: string[] = [];
  for (const line of raw.replace(/\r\n/g, '\n').split('\n')) {
    if (/^# -+ >8 -+$/.test(line)) break;
    if (!line.startsWith('#')) lines.push(line.trimEnd());
  }
  while (lines.length > 0 && lines[0] === '') lines.shift();
  while (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines.join('\n');
}

/** The text of the `BREAKING CHANGE:` footer: its paragraph, without the token. */
function breakingNote(body: string[]): string | null {
  const start = body.findIndex((line) => FOOTER.test(line));
  if (start === -1) return null;
  const paragraph = [body[start]!.replace(FOOTER, '')];
  for (const line of body.slice(start + 1)) {
    if (line === '') break;
    paragraph.push(line.trim());
  }
  return paragraph.join(' ');
}

/** Parses a cleaned message, or returns null when its header is not a Conventional Commit. */
export function parse(message: string): Commit | null {
  const [first = '', ...body] = message.split('\n');
  const pr = PR_SUFFIX.exec(first);
  const m = HEADER.exec(first.replace(PR_SUFFIX, ''));
  if (!m?.groups) return null;
  const note = breakingNote(body);
  return {
    type: m.groups.type!,
    scope: m.groups.scope ?? null,
    breaking: m.groups.bang === '!' || note !== null,
    description: m.groups.description!,
    breakingNote: note,
    pr: pr ? Number(pr[1]) : null,
  };
}

/** The faults of a cleaned message, in words that tell how to fix them. Empty when it is good. */
export function lint(message: string, opts: { allowFixup?: boolean } = {}): string[] {
  if (message === '') return ['the message is empty'];
  if (FIXUP.test(message)) {
    return opts.allowFixup ? [] : ['squash this commit into the commit that it fixes, before main'];
  }
  const lines = message.split('\n');
  const header = lines[0]!.replace(PR_SUFFIX, '');
  const faults: string[] = [];

  const commit = parse(message);
  if (!commit) {
    faults.push(`the header must be "<type>(<scope>): <description>", not "${header}"`);
    if (/^Revert "/.test(header)) {
      faults.push('for a revert, write "revert: <the header of the reverted commit>"');
    }
    if (/^Merge /.test(header)) {
      faults.push('main has a linear history: rebase, do not merge');
    }
    return faults;
  }

  if (!(commit.type in TYPES)) {
    faults.push(`unknown type "${commit.type}"; use one of: ${Object.keys(TYPES).join(', ')}`);
  }
  if (commit.scope !== null && !(commit.scope in SCOPES)) {
    faults.push(
      `unknown scope "${commit.scope}"; use one of: ${Object.keys(SCOPES).join(', ')}, ` +
        'or no scope. A new scope needs a row in SCOPES (scripts/commits.ts)',
    );
  }
  if (commit.scope === 'release' && (commit.type !== 'chore' || !SEMVER.test(commit.description))) {
    faults.push('the scope "release" is only for "chore(release): <version>" of mise run release');
  }
  if (/^[A-Z]/.test(commit.description)) {
    faults.push('start the description with a verb in the imperative, in lowercase ("add", "fix")');
  }
  if (commit.description.endsWith('.')) {
    faults.push('do not end the description with a period');
  }
  if (header.length > HEADER_LIMIT) {
    faults.push(`the header has ${header.length} characters; the limit is ${HEADER_LIMIT}`);
  }
  if (lines.length > 1 && lines[1] !== '') {
    faults.push('put a blank line between the header and the body');
  }
  if (commit.breakingNote !== null && HEADER.exec(header)?.groups?.bang !== '!') {
    faults.push('a commit with a "BREAKING CHANGE:" footer has a "!" before the colon');
  }
  return faults;
}

/** The release that a commit needs: a breaking change is major, a feature minor, a fix patch. */
export function level(commit: Commit): Level | null {
  if (commit.breaking) return 'major';
  if (commit.type === 'feat') return 'minor';
  if (commit.type === 'fix' || commit.type === 'perf' || commit.type === 'revert') return 'patch';
  return null;
}

const ORDER: (Level | null)[] = [null, 'patch', 'minor', 'major'];

/** The highest level of the commits, or null when none of them needs a release. */
export function highest(commits: Commit[]): Level | null {
  return commits.reduce<Level | null>(
    (top, c) => (ORDER.indexOf(level(c)) > ORDER.indexOf(top) ? level(c) : top),
    null,
  );
}

/** The next version after `version` for a release of `bump`. */
export function nextVersion(version: string, bump: Level): string {
  if (!SEMVER.test(version)) throw new Error(`"${version}" is not a version MAJOR.MINOR.PATCH`);
  const [major, minor, patch] = version.split('.').map(Number) as [number, number, number];
  if (bump === 'major') return `${major + 1}.0.0`;
  if (bump === 'minor') return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

/** The fault of a pull request title that hides the release level of its commits, or null. */
export function titleLevelFault(title: Commit, commits: Commit[]): string | null {
  const need = highest(commits);
  if (ORDER.indexOf(level(title)) >= ORDER.indexOf(need)) return null;
  const hint =
    need === 'major'
      ? 'a "!" before the colon'
      : need === 'minor'
        ? 'the type "feat"'
        : 'the type "fix", "perf" or "revert"';
  return `the commits need a ${need} release, but the title does not: give the title ${hint}`;
}

export type LoggedCommit = { sha: string; message: string };

function git(args: string[]): string {
  const result = Bun.spawnSync(['git', ...args], { stderr: 'pipe' });
  if (result.exitCode !== 0) {
    throw new Error(`git ${args.join(' ')} failed:\n${result.stderr.toString()}`);
  }
  return result.stdout.toString();
}

/** The commits that `git log <revs>` selects, oldest first, without merge commits. */
export function log(revs: string[]): LoggedCommit[] {
  return git(['log', '--reverse', '--no-merges', '--format=%H%x00%B%x1e', ...revs])
    .split('\x1e')
    .map((record) => record.replace(/^\n/, ''))
    .filter((record) => record !== '')
    .map((record) => {
      const [sha = '', message = ''] = record.split('\x00');
      return { sha, message: cleanMessage(message) };
    });
}

/** The revisions of a range. A push of a new branch starts at `0000…`: then only its last commit. */
function rangeRevs(range: string): string[] {
  const [from = '', to = ''] = range.split('..');
  return /^0+$/.test(from) ? ['-1', to] : [range];
}

function report(name: string, faults: string[]): boolean {
  if (faults.length === 0) return true;
  console.error(`${name}:\n${faults.map((f) => `  - ${f}`).join('\n')}`);
  return false;
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const value = (flag: string) => {
    const i = args.indexOf(flag);
    return i === -1 ? undefined : args[i + 1];
  };
  let ok = true;
  const file = value('--file');
  const title = value('--title');
  let range = value('--range');

  if (args.includes('--unpushed')) {
    const upstream = Bun.spawnSync(['git', 'rev-parse', '--abbrev-ref', '@{upstream}'], {
      stderr: 'pipe',
    });
    if (upstream.exitCode !== 0) {
      console.log('check:commits: the branch has no upstream, so no commit is unpushed');
      process.exit(0);
    }
    range = `${upstream.stdout.toString().trim()}..HEAD`;
  }

  if (file !== undefined) {
    const message = cleanMessage(await Bun.file(file).text());
    ok = report('the commit message', lint(message, { allowFixup: true }));
    if (!ok) {
      console.error(`\n${DOC} The message is in ${file}.`);
    }
    process.exit(ok ? 0 : 1);
  }

  const commits = range === undefined ? [] : log(rangeRevs(range));
  // Fixups are permitted on a pull request, which GitHub squashes, but never on main.
  const branch = git(['rev-parse', '--abbrev-ref', 'HEAD']).trim();
  const allowFixup = title !== undefined || (args.includes('--unpushed') && branch !== 'main');
  for (const c of commits) {
    ok =
      report(
        `commit ${c.sha.slice(0, 7)} "${c.message.split('\n')[0]}"`,
        lint(c.message, { allowFixup }),
      ) && ok;
  }
  if (title !== undefined) {
    const faults = lint(title);
    const parsed = parse(title);
    const all = commits.map((c) => parse(c.message)).filter((c): c is Commit => c !== null);
    const fault = parsed ? titleLevelFault(parsed, all) : null;
    if (fault) faults.push(fault);
    ok = report(`the title of the pull request "${title}"`, faults) && ok;
  }
  if (!ok) {
    console.error(`\n${DOC}`);
    process.exit(1);
  }
  const what = title !== undefined ? 'the title and ' : '';
  console.log(`commits: ${what}${commits.length} commit(s) follow Conventional Commits`);
}
