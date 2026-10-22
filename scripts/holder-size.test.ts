import { describe, expect, it } from 'bun:test';
import { blankNonCode, countCodeLines } from './holder-size';

describe('countCodeLines', () => {
  it('counts code and skips blank lines and comments', () => {
    const src = ['//! crate doc', '', '/// doc', 'fn a() {', '    // note', '    1', '}', ''].join(
      '\n',
    );
    expect(countCodeLines(src)).toBe(3);
  });

  it('counts a line that has code and a trailing comment', () => {
    expect(countCodeLines('let x = 1; // why\n')).toBe(1);
  });

  it('skips block comments, also nested ones', () => {
    expect(countCodeLines('/* a\n /* b */\n c */\nfn a() {}\n')).toBe(1);
  });

  it('ignores braces and comment markers inside strings and characters', () => {
    const src = ['fn a() {', '    let s = "}} // not a comment";', "    let c = '{';", '}'].join(
      '\n',
    );
    expect(countCodeLines(src)).toBe(4);
  });

  it('counts the lines inside a multi-line string', () => {
    expect(countCodeLines('let s = "a\nb\nc";\n')).toBe(3);
  });

  it('handles raw strings and lifetimes', () => {
    const src = ["fn a<'x>(s: &'x str) {", '    let r = r#"} " {"#;', '}'].join('\n');
    expect(countCodeLines(src)).toBe(3);
  });

  it('skips an inline #[cfg(test)] module', () => {
    const src = [
      'fn a() {}',
      '',
      '#[cfg(test)]',
      'mod tests {',
      '    use super::*;',
      '    #[test]',
      '    fn t() { let s = "}"; }',
      '}',
      'fn b() {}',
    ].join('\n');
    expect(countCodeLines(src)).toBe(2);
  });

  it('does not skip a module that is not a test module', () => {
    expect(countCodeLines('mod real {\n    fn a() {}\n}\n')).toBe(3);
  });

  it('rejects a #[cfg(test)] module in another file', () => {
    expect(() => countCodeLines('#[cfg(test)]\nmod tests;\n')).toThrow('inline');
  });
});

describe('blankNonCode', () => {
  it('keeps the line structure', () => {
    const src = '// a\nlet s = "x\ny"; /* b\nc */\n';
    expect(blankNonCode(src).split('\n')).toHaveLength(src.split('\n').length);
  });
});
