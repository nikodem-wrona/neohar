import { describe, expect, it } from 'bun:test';
import {
  cleanMessage,
  HEADER_LIMIT,
  highest,
  level,
  lint,
  nextVersion,
  parse,
  titleLevelFault,
  type Commit,
} from './commits';

const commit = (message: string): Commit => {
  const c = parse(message);
  if (!c) throw new Error(`not a commit: ${message}`);
  return c;
};

describe('parse', () => {
  it('reads the type, the scope and the description', () => {
    expect(parse('feat(ui): show the errors of commands')).toEqual({
      type: 'feat',
      scope: 'ui',
      breaking: false,
      description: 'show the errors of commands',
      breakingNote: null,
      pr: null,
    });
  });

  it('reads a header with no scope', () => {
    expect(parse('docs: write the README again')?.scope).toBeNull();
  });

  it('reads a "!" and a BREAKING CHANGE footer as a breaking change', () => {
    expect(parse('feat(api)!: remove pane.paste_confirm')?.breaking).toBe(true);
    const c = commit(
      'feat(api)!: remove pane.paste_confirm\n\nWhy.\n\nBREAKING CHANGE: clients send\npane.paste.\n\nCo-Authored-By: A <a@b.c>',
    );
    expect(c.breakingNote).toBe('clients send pane.paste.');
  });

  it('reads the number of the pull request of a squash merge', () => {
    const c = commit('fix(core): keep the holder of a moved pane (#12)');
    expect(c.pr).toBe(12);
    expect(c.description).toBe('keep the holder of a moved pane');
  });

  it('refuses a header that is not a Conventional Commit', () => {
    expect(parse('Docs site: serve it under /neohar/')).toBeNull();
    expect(parse('feat:no space')).toBeNull();
    expect(parse('feat:  two spaces')).toBeNull();
    expect(parse('neohar 1.0.0')).toBeNull();
  });
});

describe('lint', () => {
  it('accepts a good message with a body and trailers', () => {
    const message =
      'perf(term): build a frame in one pass\n\nThe bench: 1.9 ms to 1.2 ms.\n\nCo-Authored-By: A <a@b.c>';
    expect(lint(message)).toEqual([]);
  });

  it('names an unknown type and an unknown scope', () => {
    expect(lint('feature: add x')[0]).toContain('unknown type "feature"');
    expect(lint('feat(board): add x')[0]).toContain('unknown scope "board"');
  });

  it('wants a lowercase verb and no period', () => {
    expect(lint('fix: Fix the input ids')).toHaveLength(1);
    expect(lint('fix: fix the input ids.')).toEqual(['do not end the description with a period']);
  });

  it('limits the header, without the suffix of a pull request', () => {
    const long = `fix: ${'a'.repeat(HEADER_LIMIT - 5)}`;
    expect(lint(long)).toEqual([]);
    expect(lint(`${long} (#123)`)).toEqual([]);
    expect(lint(`${long}a`)[0]).toContain('the limit is 72');
  });

  it('wants a blank line after the header', () => {
    expect(lint('fix: a\nbody')).toEqual(['put a blank line between the header and the body']);
  });

  it('wants a "!" with a BREAKING CHANGE footer', () => {
    expect(lint('feat: a\n\nBREAKING CHANGE: b')[0]).toContain('"!"');
    expect(lint('feat!: a\n\nBREAKING CHANGE: b')).toEqual([]);
    expect(lint('fix: a\n\nNot a BREAKING CHANGE: here')).toEqual([]);
  });

  it('keeps the scope "release" for the release commit', () => {
    expect(lint('chore(release): 1.2.0')).toEqual([]);
    expect(lint('feat(release): 1.2.0')).toHaveLength(1);
    expect(lint('chore(release): make a release')).toHaveLength(1);
  });

  it('tells how to write a revert and refuses a merge', () => {
    expect(lint('Revert "feat: a"')[1]).toContain('revert: <the header');
    expect(lint('revert: feat(ui): add x\n\nThis reverts commit abc.')).toEqual([]);
    expect(lint("Merge branch 'x'")[1]).toContain('linear history');
  });

  it('permits a fixup only when asked', () => {
    expect(lint('fixup! feat: a', { allowFixup: true })).toEqual([]);
    expect(lint('fixup! feat: a')[0]).toContain('squash this commit');
  });

  it('refuses an empty message', () => {
    expect(lint('')).toEqual(['the message is empty']);
  });
});

describe('cleanMessage', () => {
  it('removes comments, the diff of git commit -v and blank edges', () => {
    const raw = [
      '',
      'fix: a',
      '',
      'Body.  ',
      '# Please enter the commit message',
      '',
      '# ------------------------ >8 ------------------------',
      'diff --git a/x b/x',
    ].join('\n');
    expect(cleanMessage(raw)).toBe('fix: a\n\nBody.');
  });
});

describe('versions', () => {
  it('gives each commit its level', () => {
    expect(level(commit('feat!: a'))).toBe('major');
    expect(level(commit('refactor(api)!: a'))).toBe('major');
    expect(level(commit('feat: a'))).toBe('minor');
    expect(level(commit('fix: a'))).toBe('patch');
    expect(level(commit('perf: a'))).toBe('patch');
    expect(level(commit('revert: fix: a'))).toBe('patch');
    expect(level(commit('docs: a'))).toBeNull();
    expect(level(commit('chore(release): 1.0.0'))).toBeNull();
  });

  it('takes the highest level of a list', () => {
    expect(highest([commit('docs: a'), commit('fix: b'), commit('feat: c')])).toBe('minor');
    expect(highest([commit('docs: a'), commit('test: b')])).toBeNull();
  });

  it('counts the next version', () => {
    expect(nextVersion('1.4.2', 'major')).toBe('2.0.0');
    expect(nextVersion('1.4.2', 'minor')).toBe('1.5.0');
    expect(nextVersion('1.4.2', 'patch')).toBe('1.4.3');
    expect(() => nextVersion('1.4', 'patch')).toThrow('MAJOR.MINOR.PATCH');
  });

  it('wants the title of a pull request to keep the level of its commits', () => {
    const commits = [commit('fix: a'), commit('feat: b')];
    expect(titleLevelFault(commit('feat(ui): c'), commits)).toBeNull();
    expect(titleLevelFault(commit('fix(ui): c'), commits)).toContain('the type "feat"');
    expect(titleLevelFault(commit('feat: c'), [commit('feat!: d')])).toContain('"!"');
    expect(titleLevelFault(commit('docs: c'), [commit('test: d')])).toBeNull();
  });
});
