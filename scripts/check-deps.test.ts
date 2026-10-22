import { describe, expect, it } from 'bun:test';
import { RULES, pinProblem, problems, treeNames } from './check-deps';

describe('treeNames', () => {
  it('reads the crate names of `cargo tree --prefix none`', () => {
    const out =
      'nh-hold v0.0.0 (/repo/crates/nh-hold)\nserde v1.0.0\nserde_derive v1.0.0 (proc-macro) (*)\n';
    expect([...treeNames(out)]).toEqual(['nh-hold', 'serde', 'serde_derive']);
  });
});

describe('problems', () => {
  const holder = RULES['nh-hold'];

  it('accepts a crate that follows its row', () => {
    expect(
      problems('nh-hold', holder, ['nh-proto'], new Set(['nh-hold', 'nh-proto', 'rustix'])),
    ).toEqual([]);
  });

  it('rejects a workspace dependency that the row does not allow', () => {
    expect(problems('nh-hold', holder, ['nh-term'], new Set())).toEqual([
      'nh-hold: may not depend on nh-term',
    ]);
  });

  it('rejects a forbidden crate anywhere in the tree', () => {
    expect(problems('nh-hold', holder, [], new Set(['mio', 'tokio']))).toEqual([
      'nh-hold: tokio is in its dependency tree',
    ]);
  });

  it('rejects a crate that has no row', () => {
    expect(problems('nh-new', undefined, [], new Set())[0]).toContain('no row in RULES');
  });

  it('lets an `any` row depend on every workspace crate', () => {
    expect(
      problems('nh-bench', { allow: 'any', forbid: [] }, ['nh-core', 'nh-term'], new Set()),
    ).toEqual([]);
  });
});

describe('pinProblem', () => {
  const lock = (v: string) => `[[package]]\nname = "wasm-bindgen"\nversion = "${v}"\n`;
  const mise = (v: string) => `[tools]\n"cargo:wasm-bindgen-cli" = "${v}"\n`;

  it('passes when Cargo.lock has no wasm-bindgen', () => {
    expect(
      pinProblem('[[package]]\nname = "serde"\nversion = "1.0.0"\n', mise('0.2.129')),
    ).toBeNull();
  });

  it('passes when the versions are equal', () => {
    expect(pinProblem(lock('0.2.129'), mise('0.2.129'))).toBeNull();
  });

  it('fails when the versions differ', () => {
    expect(pinProblem(lock('0.2.130'), mise('0.2.129'))).toContain('0.2.130');
  });
});
