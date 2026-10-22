// The version of neohar, its changelog and its releases. Semantic Versioning 2.0.0.
//
// `[workspace.package] version` in Cargo.toml is the version. Every other file of `versionFiles`
// repeats it, and only this script changes them. The commits since the last tag `vX.Y.Z` give the
// next version (scripts/commits.ts): a breaking change makes a major release, a `feat` a minor
// release, and a `fix`, `perf` or `revert` a patch release. Other types make no release.
//
// Usage:
//   bun scripts/release.ts                    make the release commit and its tag (mise run release)
//   bun scripts/release.ts --dry-run          show the next version and its changelog section
//   bun scripts/release.ts --check            the files agree on the version (mise run check:version)
//   bun scripts/release.ts --check --tag <t>  also: <t> is the release commit of that version, on main
//   bun scripts/release.ts --notes <version>  the changelog section of a version, for the GitHub release
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { type Commit, highest, lint, log, nextVersion, parse } from './commits';

const SEMVER = /^\d+\.\d+\.\d+$/;
const TAG_GLOB = 'v[0-9]*.[0-9]*.[0-9]*';

/** The files that hold the version, relative to the root: Cargo.toml first, as the source. */
export function versionFiles(root: string): string[] {
  const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')) as {
    workspaces: string[];
  };
  const workspaces = pkg.workspaces.flatMap((pattern) => [
    ...new Bun.Glob(`${pattern}/package.json`).scanSync({ cwd: root }),
  ]);
  return [
    'Cargo.toml',
    'Cargo.lock',
    'bun.lock',
    'apps/desktop/src-tauri/tauri.conf.json',
    'package.json',
    ...workspaces.sort(),
    'website/package.json',
  ];
}

const CARGO_TOML = /(\[workspace\.package\]\n(?:[^[\n].*\n|\n)*?version = ")([^"]+)(")/;
const JSON_TOP = /^( {2}"version": ")([^"]+)(")/m;

/** The blocks of the workspace members in Cargo.lock: a `[[package]]` with no `source`. */
function lockBlocks(text: string): { text: string; member: boolean }[] {
  return text.split(/(?=^\[\[package\]\]$)/m).map((block) => ({
    text: block,
    member: block.startsWith('[[package]]') && !/^source = /m.test(block),
  }));
}

/** The part of bun.lock with the workspace packages: before its `"packages"` object. */
function bunWorkspaces(text: string): [string, string] {
  const at = text.indexOf('\n  "packages": {');
  return at === -1 ? [text, ''] : [text.slice(0, at), text.slice(at)];
}

/** Every version that a file of `versionFiles` holds. */
export function versionsIn(path: string, text: string): string[] {
  if (path === 'Cargo.toml') return [CARGO_TOML.exec(text)?.[2]].filter((v) => v !== undefined);
  if (path === 'Cargo.lock') {
    return lockBlocks(text)
      .filter((b) => b.member)
      .flatMap((b) => [/^version = "([^"]+)"$/m.exec(b.text)?.[1] ?? '(none)']);
  }
  if (path === 'bun.lock') {
    return [...bunWorkspaces(text)[0].matchAll(/"version": "([^"]+)"/g)].map((m) => m[1]!);
  }
  return [JSON_TOP.exec(text)?.[2]].filter((v) => v !== undefined);
}

/** The text of a file of `versionFiles` with the version `to` in place of `from`. */
export function withVersion(path: string, text: string, from: string, to: string): string {
  const swap = (_: string, head: string, version: string, tail: string) =>
    version === from ? `${head}${to}${tail}` : `${head}${version}${tail}`;
  let out: string;
  if (path === 'Cargo.toml') {
    out = text.replace(CARGO_TOML, swap);
  } else if (path === 'Cargo.lock') {
    out = lockBlocks(text)
      .map((b) => (b.member ? b.text.replace(/^(version = ")([^"]+)(")$/m, swap) : b.text))
      .join('');
  } else if (path === 'bun.lock') {
    const [workspaces, rest] = bunWorkspaces(text);
    out = workspaces.replace(/("version": ")([^"]+)(")/g, swap) + rest;
  } else {
    out = text.replace(JSON_TOP, swap);
  }
  const left = versionsIn(path, out).filter((v) => v !== to);
  if (left.length > 0 || versionsIn(path, out).length === 0) {
    throw new Error(`${path}: cannot set the version ${to} (found ${left.join(', ') || 'none'})`);
  }
  return out;
}

/** The faults of the version files: each must hold `expected`, and only it. */
export function versionFaults(found: Map<string, string[]>, expected: string): string[] {
  const faults: string[] = [];
  if (!SEMVER.test(expected)) faults.push(`Cargo.toml: "${expected}" is not MAJOR.MINOR.PATCH`);
  for (const [path, versions] of found) {
    if (versions.length === 0) faults.push(`${path}: no version`);
    const other = [...new Set(versions.filter((v) => v !== expected))];
    if (other.length > 0) faults.push(`${path}: ${other.join(', ')}, not ${expected}`);
  }
  return faults;
}

/** The faults of CHANGELOG.md for the version `version`. */
export function changelogFaults(changelog: string, version: string): string[] {
  const faults: string[] = [];
  if (!/^## \[Unreleased\]$/m.test(changelog)) faults.push('CHANGELOG.md: no "## [Unreleased]"');
  const v = version.replace(/\./g, '\\.');
  if (!new RegExp(`^## \\[${v}\\] - \\d{4}-\\d{2}-\\d{2}$`, 'm').test(changelog)) {
    faults.push(`CHANGELOG.md: no "## [${version}] - YYYY-MM-DD"`);
  }
  if (!new RegExp(`^\\[${v}\\]: `, 'm').test(changelog)) {
    faults.push(`CHANGELOG.md: no link "[${version}]: ..."`);
  }
  return faults;
}

export type ReleaseCommit = Commit & { sha: string };

const SECTIONS: [string, (c: Commit) => boolean][] = [
  ['Breaking changes', (c) => c.breaking],
  ['Added', (c) => !c.breaking && c.type === 'feat'],
  ['Changed', (c) => !c.breaking && (c.type === 'perf' || c.type === 'revert')],
  ['Fixed', (c) => !c.breaking && c.type === 'fix'],
];

function entry(c: ReleaseCommit, repo: string): string {
  const scope = c.scope ? `**${c.scope}:** ` : '';
  const text = c.type === 'revert' ? `revert ${c.description}` : c.description;
  const note = c.breaking && c.breakingNote ? `. ${c.breakingNote}` : '';
  const link =
    c.pr !== null
      ? `[#${c.pr}](${repo}/pull/${c.pr})`
      : `[${c.sha.slice(0, 7)}](${repo}/commit/${c.sha})`;
  return `- ${scope}${text}${note} (${link})`;
}

/** The body of a release section: the notes from Unreleased, then one list for each kind. */
export function renderSection(commits: ReleaseCommit[], notes: string, repo: string): string {
  const parts = notes.trim() === '' ? [] : [notes.trim()];
  for (const [title, test] of SECTIONS) {
    const entries = commits.filter(test).map((c) => entry(c, repo));
    if (entries.length > 0) parts.push(`### ${title}\n\n${entries.join('\n')}`);
  }
  return parts.join('\n\n');
}

/** The notes that the maintainer wrote under `## [Unreleased]`. */
export function unreleasedNotes(changelog: string): string {
  const m = /^## \[Unreleased\]\n([\s\S]*?)(?=^## \[|^\[Unreleased\]: )/m.exec(changelog);
  return m?.[1]?.trim() ?? '';
}

/** CHANGELOG.md with the section of `version` under an empty `## [Unreleased]`. */
export function insertRelease(
  changelog: string,
  release: { version: string; previous: string; date: string; body: string; repo: string },
): string {
  const { version, previous, date, body, repo } = release;
  const section = `## [Unreleased]\n\n## [${version}] - ${date}\n\n${body}\n\n`;
  const out = changelog
    .replace(/^## \[Unreleased\]\n[\s\S]*?(?=^## \[|^\[Unreleased\]: )/m, section)
    .replace(
      /^\[Unreleased\]: .*$/m,
      `[Unreleased]: ${repo}/compare/v${version}...HEAD\n` +
        `[${version}]: ${repo}/compare/v${previous}...v${version}`,
    );
  if (changelogFaults(out, version).length > 0) {
    throw new Error('CHANGELOG.md: cannot find "## [Unreleased]" and its link');
  }
  return out;
}

/** The section of a version, for the GitHub release, with the compare link when there is one. */
export function releaseNotes(changelog: string, version: string): string {
  const v = version.replace(/\./g, '\\.');
  const m = new RegExp(`^## \\[${v}\\] - .*\\n([\\s\\S]*?)(?=^## \\[|^\\[Unreleased\\]: )`, 'm');
  const body = m.exec(changelog)?.[1]?.trim();
  if (body === undefined) throw new Error(`CHANGELOG.md: no section for ${version}`);
  const link = new RegExp(`^\\[${v}\\]: (.*/compare/.*)$`, 'm').exec(changelog)?.[1];
  return link ? `${body}\n\n**Full changelog:** ${link}\n` : `${body}\n`;
}

function git(args: string[], opts: { check?: boolean } = {}): { ok: boolean; out: string } {
  const result = Bun.spawnSync(['git', ...args], { stderr: 'pipe' });
  if (opts.check !== false && result.exitCode !== 0) {
    throw new Error(`git ${args.join(' ')} failed:\n${result.stderr.toString()}`);
  }
  return { ok: result.exitCode === 0, out: result.stdout.toString().trim() };
}

function fail(faults: string[]): never {
  console.error(faults.join('\n'));
  process.exit(1);
}

if (import.meta.main) {
  const root = join(import.meta.dir, '..');
  process.chdir(root);
  const args = process.argv.slice(2);
  const value = (flag: string) => {
    const i = args.indexOf(flag);
    return i === -1 ? undefined : args[i + 1];
  };
  const read = (path: string) => readFileSync(join(root, path), 'utf8');
  const files = versionFiles(root);
  const cargo = Bun.TOML.parse(read('Cargo.toml')) as {
    workspace: { package: { version: string; repository: string } };
  };
  const version = cargo.workspace.package.version;
  const repo = cargo.workspace.package.repository;
  const found = new Map(files.map((f) => [f, versionsIn(f, read(f))]));

  const notesFor = value('--notes');
  if (notesFor !== undefined) {
    process.stdout.write(releaseNotes(read('CHANGELOG.md'), notesFor));
    process.exit(0);
  }

  const lastTag = () => {
    const tag = git(['describe', '--tags', '--abbrev=0', '--match', TAG_GLOB], { check: false });
    return tag.ok ? tag.out : null;
  };
  const faults = [
    ...versionFaults(found, version),
    ...changelogFaults(read('CHANGELOG.md'), version),
  ];

  if (args.includes('--check')) {
    const tag = value('--tag');
    const last = lastTag();
    if (last === null) {
      faults.push(`no tag ${TAG_GLOB} before HEAD: run "git fetch --tags" (CI: fetch-depth: 0)`);
    } else if (last !== `v${version}`) {
      faults.push(
        `the version is ${version}, but the last release is ${last}. ` +
          'Only "mise run release" changes the version',
      );
    }
    if (tag !== undefined) {
      const header = git(['log', '-1', '--format=%s', 'HEAD']).out;
      if (tag !== `v${version}`) faults.push(`the tag ${tag} is not v${version}`);
      if (header !== `chore(release): ${version}`) {
        faults.push(`the tag ${tag} is on "${header}", not on "chore(release): ${version}"`);
      }
      if (!git(['merge-base', '--is-ancestor', 'HEAD', 'origin/main'], { check: false }).ok) {
        faults.push(`the tag ${tag} is not on origin/main`);
      }
    }
    if (faults.length > 0) fail(faults);
    console.log(`check:version: ${files.length} files hold ${version}, the version of ${last}`);
    process.exit(0);
  }

  // The release.
  const dryRun = args.includes('--dry-run');
  if (faults.length > 0) fail(faults);
  git(['fetch', '--quiet', '--tags', 'origin', 'main']);
  const last = lastTag();
  if (last !== `v${version}`) {
    fail([`the version is ${version}, but the last tag is ${last ?? '(none)'}`]);
  }
  if (!dryRun) {
    const problems: string[] = [];
    if (git(['rev-parse', '--abbrev-ref', 'HEAD']).out !== 'main') problems.push('check out main');
    if (git(['status', '--porcelain']).out !== '') problems.push('the working tree is not clean');
    if (git(['rev-parse', 'HEAD']).out !== git(['rev-parse', 'origin/main']).out) {
      problems.push('main is not equal to origin/main: pull or push first');
    }
    if (problems.length > 0) fail(problems.map((p) => `release: ${p}`));
  }

  const commits: ReleaseCommit[] = [];
  for (const c of log([`${last}..HEAD`])) {
    const parsed = parse(c.message);
    if (parsed === null || lint(c.message).length > 0) {
      console.warn(`release: not a Conventional Commit, left out: ${c.sha.slice(0, 7)}`);
    } else {
      commits.push({ ...parsed, sha: c.sha });
    }
  }
  const bump = highest(commits);
  if (bump === null) {
    fail([
      `release: no commit since ${last} needs a release.`,
      'A release needs a feat, a fix, a perf, a revert or a breaking change.',
    ]);
  }
  const next = nextVersion(version, bump);
  if (git(['rev-parse', '--quiet', '--verify', `refs/tags/v${next}`], { check: false }).ok) {
    fail([`release: the tag v${next} exists already`]);
  }
  const changelog = read('CHANGELOG.md');
  const body = renderSection(commits, unreleasedNotes(changelog), repo);
  const date = new Date().toISOString().slice(0, 10);

  if (dryRun) {
    console.log(`${version} -> ${next} (${bump}), ${commits.length} commits since ${last}\n`);
    console.log(`## [${next}] - ${date}\n\n${body}`);
    process.exit(0);
  }

  for (const f of files) writeFileSync(join(root, f), withVersion(f, read(f), version, next));
  writeFileSync(
    join(root, 'CHANGELOG.md'),
    insertRelease(changelog, { version: next, previous: version, date, body, repo }),
  );
  git(['add', '--', ...files, 'CHANGELOG.md']);
  git(['commit', '--quiet', '-m', `chore(release): ${next}`]);
  git(['tag', '-a', `v${next}`, '-m', `neohar ${next}`]);
  console.log(
    [
      `Made the commit and the tag of neohar ${next} (${bump}). Next:`,
      '',
      '  mise run check && mise run test',
      `  git push --atomic origin main v${next}`,
      '',
      'The tag starts .github/workflows/release.yml. It runs CI on the tag, then it publishes',
      'the GitHub release with the section of CHANGELOG.md.',
      '',
      `To undo before the push: git tag -d v${next} && git reset --keep HEAD~1`,
    ].join('\n'),
  );
}
