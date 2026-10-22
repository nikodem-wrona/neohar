import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { parse } from './commits';
import {
  changelogFaults,
  insertRelease,
  releaseNotes,
  renderSection,
  type ReleaseCommit,
  unreleasedNotes,
  versionFaults,
  versionFiles,
  versionsIn,
  withVersion,
} from './release';

const root = join(import.meta.dir, '..');
const REPO = 'https://github.com/o/neohar';

const rc = (message: string, sha = 'abcdef1234567890'): ReleaseCommit => {
  const c = parse(message);
  if (!c) throw new Error(`not a commit: ${message}`);
  return { ...c, sha };
};

const CHANGELOG = `# Changelog

Intro.

## [Unreleased]

Quit the app before you update.

## [1.0.0] - 2026-10-21

The first release.

[Unreleased]: ${REPO}/compare/v1.0.0...HEAD
[1.0.0]: ${REPO}/releases/tag/v1.0.0
`;

describe('version files', () => {
  it('finds every file of the repository and reads one version from each', () => {
    const files = versionFiles(root);
    expect(files[0]).toBe('Cargo.toml');
    expect(files).toContain('apps/desktop/src-tauri/tauri.conf.json');
    expect(files).toContain('packages/ui/package.json');
    expect(files).toContain('website/package.json');
    const version = (
      Bun.TOML.parse(readFileSync(join(root, 'Cargo.toml'), 'utf8')) as {
        workspace: { package: { version: string } };
      }
    ).workspace.package.version;
    const found = new Map(
      files.map((f) => [f, versionsIn(f, readFileSync(join(root, f), 'utf8'))]),
    );
    expect(versionFaults(found, version)).toEqual([]);
  });

  it('changes the version of Cargo.toml and not rust-version', () => {
    const toml = '[workspace]\n\n[workspace.package]\nversion = "1.0.0"\nrust-version = "1.99"\n';
    expect(withVersion('Cargo.toml', toml, '1.0.0', '1.1.0')).toBe(
      '[workspace]\n\n[workspace.package]\nversion = "1.1.0"\nrust-version = "1.99"\n',
    );
  });

  it('changes only the workspace members of Cargo.lock', () => {
    const lock = [
      'version = 4',
      '',
      '[[package]]',
      'name = "nh"',
      'version = "1.0.0"',
      '',
      '[[package]]',
      'name = "serde"',
      'version = "1.0.0"',
      'source = "registry+https://github.com/rust-lang/crates.io-index"',
      '',
    ].join('\n');
    const out = withVersion('Cargo.lock', lock, '1.0.0', '2.0.0');
    expect(versionsIn('Cargo.lock', out)).toEqual(['2.0.0']);
    expect(out).toContain('name = "serde"\nversion = "1.0.0"');
    expect(out.split('\n')[0]).toBe('version = 4');
  });

  it('changes only the workspaces of bun.lock', () => {
    const lock =
      '{\n  "workspaces": {\n    "a": {\n      "name": "a",\n      "version": "1.0.0",\n    },\n  },\n' +
      '  "packages": {\n    "x": ["x@1.0.0", "", { "version": "1.0.0" }],\n  },\n}\n';
    const out = withVersion('bun.lock', lock, '1.0.0', '1.0.1');
    expect(versionsIn('bun.lock', out)).toEqual(['1.0.1']);
    expect(out).toContain('{ "version": "1.0.0" }');
  });

  it('changes the top-level version of a JSON file only', () => {
    const json =
      '{\n  "name": "a",\n  "version": "1.0.0",\n  "x": {\n    "version": "1.0.0"\n  }\n}\n';
    const out = withVersion('package.json', json, '1.0.0', '1.0.1');
    expect(out).toContain('  "version": "1.0.1",');
    expect(out).toContain('    "version": "1.0.0"');
  });

  it('refuses a file that holds another version', () => {
    expect(() =>
      withVersion('package.json', '{\n  "version": "0.9.0"\n}', '1.0.0', '1.0.1'),
    ).toThrow('cannot set the version');
  });

  it('names each file that disagrees', () => {
    const found = new Map([
      ['Cargo.toml', ['1.0.0']],
      ['package.json', ['1.1.0']],
      ['bun.lock', []],
    ]);
    expect(versionFaults(found, '1.0.0')).toEqual([
      'package.json: 1.1.0, not 1.0.0',
      'bun.lock: no version',
    ]);
  });
});

describe('changelog', () => {
  it('writes the sections from the commits, and the notes first', () => {
    const body = renderSection(
      [
        rc('feat(ui): x', 'aaaaaaa000'),
        rc('docs: hidden'),
        rc('fix: y (#7)'),
        rc('perf(term): z'),
        rc('feat(api)!: drop m\n\nBREAKING CHANGE: use n', 'bbbbbbb000'),
        rc('revert: feat(ui): w'),
      ],
      'Quit the app first.',
      REPO,
    );
    expect(body).toBe(
      [
        'Quit the app first.',
        '',
        '### Breaking changes',
        '',
        `- **api:** drop m. use n ([bbbbbbb](${REPO}/commit/bbbbbbb000))`,
        '',
        '### Added',
        '',
        `- **ui:** x ([aaaaaaa](${REPO}/commit/aaaaaaa000))`,
        '',
        '### Changed',
        '',
        `- **term:** z ([abcdef1](${REPO}/commit/abcdef1234567890))`,
        `- revert feat(ui): w ([abcdef1](${REPO}/commit/abcdef1234567890))`,
        '',
        '### Fixed',
        '',
        `- y ([#7](${REPO}/pull/7))`,
      ].join('\n'),
    );
  });

  it('reads the notes under Unreleased', () => {
    expect(unreleasedNotes(CHANGELOG)).toBe('Quit the app before you update.');
    expect(unreleasedNotes(CHANGELOG.replace('Quit the app before you update.\n\n', ''))).toBe('');
  });

  it('puts a release under an empty Unreleased, with its compare link', () => {
    const out = insertRelease(CHANGELOG, {
      version: '1.1.0',
      previous: '1.0.0',
      date: '2026-11-01',
      body: '### Added\n\n- x',
      repo: REPO,
    });
    expect(out).toBe(`# Changelog

Intro.

## [Unreleased]

## [1.1.0] - 2026-11-01

### Added

- x

## [1.0.0] - 2026-10-21

The first release.

[Unreleased]: ${REPO}/compare/v1.1.0...HEAD
[1.1.0]: ${REPO}/compare/v1.0.0...v1.1.0
[1.0.0]: ${REPO}/releases/tag/v1.0.0
`);
    expect(changelogFaults(out, '1.1.0')).toEqual([]);
    expect(releaseNotes(out, '1.1.0')).toBe(
      `### Added\n\n- x\n\n**Full changelog:** ${REPO}/compare/v1.0.0...v1.1.0\n`,
    );
    expect(releaseNotes(out, '1.0.0')).toBe('The first release.\n');
  });

  it('names a missing section or link', () => {
    expect(changelogFaults(CHANGELOG, '1.2.0')).toEqual([
      'CHANGELOG.md: no "## [1.2.0] - YYYY-MM-DD"',
      'CHANGELOG.md: no link "[1.2.0]: ..."',
    ]);
    expect(() => releaseNotes(CHANGELOG, '1.2.0')).toThrow('no section');
  });

  it('agrees with CHANGELOG.md of the repository', () => {
    const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
    expect(changelogFaults(changelog, '1.0.0')).toEqual([]);
    expect(releaseNotes(changelog, '1.0.0')).toContain('The first public release.');
  });
});
