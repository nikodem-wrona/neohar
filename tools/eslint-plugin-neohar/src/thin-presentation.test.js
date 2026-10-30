// @ts-check
import { describe, expect, it } from 'bun:test';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint, RuleTester } from 'eslint';
import { builtinRules } from 'eslint/use-at-your-own-risk';
import {
  SYNTAX_SELECTORS,
  importOptions,
  timerGlobals,
  timerProperties,
} from './thin-presentation.js';

RuleTester.describe = describe;
RuleTester.it = it;

const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
});

/** @param {string} name */
const builtin = (name) => {
  const rule = builtinRules.get(name);
  if (!rule) throw new Error(`ESLint has no rule ${name}`);
  return rule;
};

// Each rule below runs with the exact options that eslint.config.js uses.

ruleTester.run('no-restricted-imports (packages/ui)', builtin('no-restricted-imports'), {
  valid: [
    { code: "import { useState } from 'react';", options: [importOptions()] },
    { code: "import { createRoot } from 'react-dom/client';", options: [importOptions()] },
    { code: "import { connect } from '@neohar/transport';", options: [importOptions()] },
    { code: "import { decode } from '@neohar/proto-wasm';", options: [importOptions()] },
    { code: "import { a } from './a';", options: [importOptions()] },
    { code: "import { b } from '../b/c';", options: [importOptions()] },
    { code: "import { test } from 'bun:test';", options: [importOptions(['bun:test'])] },
  ],
  invalid: [
    { code: "import { marked } from 'marked';", options: [importOptions()], errors: 1 },
    { code: "import { createSignal } from 'solid-js';", options: [importOptions()], errors: 1 },
    { code: "import { flushSync } from 'react-dom';", options: [importOptions()], errors: 1 },
    { code: "import { sortBy } from 'lodash/sortBy';", options: [importOptions()], errors: 1 },
    { code: "import x from '@neohar/ui';", options: [importOptions()], errors: 1 },
    { code: "import x from '@neohar/transport/internal';", options: [importOptions()], errors: 1 },
    { code: "import { readFile } from 'node:fs';", options: [importOptions()], errors: 1 },
    { code: "import { test } from 'bun:test';", options: [importOptions()], errors: 1 },
    { code: "export { x } from 'date-fns';", options: [importOptions()], errors: 1 },
  ],
});

ruleTester.run('no-restricted-globals (timers)', builtin('no-restricted-globals'), {
  valid: [
    { code: 'setTimeout(f, 10);', options: timerGlobals(['setInterval']) },
    { code: 'const setInterval = 1;', options: timerGlobals(['setInterval']) },
    { code: 'requestAnimationFrame(f);', options: timerGlobals(['setInterval', 'setTimeout']) },
  ],
  invalid: [
    { code: 'setInterval(f, 10);', options: timerGlobals(['setInterval']), errors: 1 },
    { code: 'setTimeout(f, 10);', options: timerGlobals(['setInterval', 'setTimeout']), errors: 1 },
    { code: 'const t = setInterval;', options: timerGlobals(['setInterval']), errors: 1 },
  ],
});

ruleTester.run('no-restricted-properties (timers)', builtin('no-restricted-properties'), {
  valid: [
    { code: 'window.setTimeout(f, 10);', options: timerProperties(['setInterval']) },
    { code: 'worker.setInterval(f, 10);', options: timerProperties(['setInterval']) },
  ],
  invalid: [
    { code: 'window.setInterval(f, 10);', options: timerProperties(['setInterval']), errors: 1 },
    {
      code: 'globalThis.setInterval(f, 10);',
      options: timerProperties(['setInterval']),
      errors: 1,
    },
    { code: 'self.setTimeout(f, 10);', options: timerProperties(['setTimeout']), errors: 1 },
  ],
});

ruleTester.run('no-restricted-syntax (sorting, locale, Intl)', builtin('no-restricted-syntax'), {
  valid: [
    { code: 'const n = items.length;', options: SYNTAX_SELECTORS },
    { code: 'const s = props.sort;', options: SYNTAX_SELECTORS },
    { code: 'const d = new Date(ms).toISOString();', options: SYNTAX_SELECTORS },
    { code: 'const t = String(n);', options: SYNTAX_SELECTORS },
  ],
  invalid: [
    { code: 'items.sort();', options: SYNTAX_SELECTORS, errors: 1 },
    { code: 'items.toSorted((a, b) => a - b);', options: SYNTAX_SELECTORS, errors: 1 },
    { code: "a.localeCompare('b');", options: SYNTAX_SELECTORS, errors: 1 },
    { code: 'n.toLocaleString();', options: SYNTAX_SELECTORS, errors: 1 },
    { code: 'd.toLocaleDateString();', options: SYNTAX_SELECTORS, errors: 1 },
    { code: 'd.toLocaleTimeString();', options: SYNTAX_SELECTORS, errors: 1 },
    { code: "items['sort']();", options: SYNTAX_SELECTORS, errors: 1 },
    { code: "const f = new Intl.NumberFormat('en');", options: SYNTAX_SELECTORS, errors: 1 },
    { code: 'const f = globalThis.Intl;', options: SYNTAX_SELECTORS, errors: 1 },
  ],
});

// RuleTester cannot apply file-scoped configuration, so these cases lint virtual files with the
// real eslint.config.js and check that each rule applies to the right paths.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
// These tests lint the text of files that do not exist. The type-aware rule needs a file of a
// tsconfig project, so the tests turn it off. `bun run lint` checks it on the real files.
const eslint = new ESLint({
  cwd: root,
  overrideConfigFile: join(root, 'eslint.config.js'),
  overrideConfig: [
    {
      files: ['**/*.ts', '**/*.tsx'],
      languageOptions: { parserOptions: { projectService: false } },
      rules: { '@typescript-eslint/no-floating-promises': 'off' },
    },
  ],
});

/**
 * @param {string} file path relative to the repository root
 * @param {string} code
 * @returns {Promise<string[]>} the rule ids that reported
 */
async function lint(file, code) {
  const [result] = await eslint.lintText(code, { filePath: join(root, file) });
  return (result?.messages ?? []).map((m) => m.ruleId ?? `parse error: ${m.message}`);
}

describe('eslint.config.js scoping', () => {
  it('forbids innerHTML in the UI and allows it in RichHtml.tsx only', async () => {
    const code = 'export const f = (el: HTMLElement, h: string) => { el.innerHTML = h; };';
    expect(await lint('packages/ui/src/cards/Card.ts', code)).toEqual(['neohar/no-inner-html']);
    expect(await lint('packages/transport/src/x.ts', code)).toEqual(['neohar/no-inner-html']);
    expect(await lint('packages/ui/src/rich/RichHtml.tsx', code)).toEqual([]);
  });

  it('restricts imports in packages/ui only', async () => {
    const code = "import { marked } from 'marked';\nexport const x = marked;";
    expect(await lint('packages/ui/src/a.ts', code)).toEqual(['no-restricted-imports']);
    expect(await lint('packages/transport/src/a.ts', code)).toEqual([]);
    expect(
      await lint('packages/ui/src/a.ts', "import { useState } from 'react';\nexport { useState };"),
    ).toEqual([]);
  });

  it('forbids the synthetic beforeinput of React in the UI', async () => {
    const code = 'export const A = (f: () => void) => <textarea onBeforeInput={f} />;';
    expect(await lint('packages/ui/src/pane/A.tsx', code)).toEqual([
      'neohar/no-synthetic-beforeinput',
    ]);
  });

  it('checks the rules of hooks in the UI', async () => {
    const code =
      "import { useEffect, useState } from 'react';\n" +
      'export function A(p: { on: boolean; x: number }) {\n' +
      '  if (p.on) useState(0);\n' +
      '  useEffect(() => console.log(p.x), []);\n' +
      '  return null;\n' +
      '}';
    expect(await lint('packages/ui/src/pane/A.tsx', code)).toEqual([
      'react-hooks/rules-of-hooks',
      'react-hooks/exhaustive-deps',
    ]);
  });

  it('lets UI tests import bun:test', async () => {
    const code = "import { it } from 'bun:test';\nit('x', () => {});";
    expect(await lint('packages/ui/src/a.test.ts', code)).toEqual([]);
    expect(await lint('packages/ui/src/a.ts', code)).toEqual(['no-restricted-imports']);
  });

  it('forbids setInterval everywhere and setTimeout outside timing.ts', async () => {
    const interval = 'export const t = setInterval(() => {}, 1);';
    const timeout = 'export const t = setTimeout(() => {}, 1);';
    expect(await lint('packages/transport/src/a.ts', interval)).toEqual(['no-restricted-globals']);
    expect(await lint('packages/ui/src/lib/timing.ts', interval)).toEqual([
      'no-restricted-globals',
    ]);
    expect(await lint('packages/ui/src/a.ts', timeout)).toEqual(['no-restricted-globals']);
    expect(await lint('packages/ui/src/lib/timing.ts', timeout)).toEqual([]);
    expect(await lint('packages/ui/src/a.test.ts', timeout)).toEqual([]);
  });

  it('forbids sorting, locale formatting and Intl outside the surface directory', async () => {
    const sort = 'export const s = (a: number[]) => a.sort();';
    expect(await lint('packages/ui/src/sidebar/List.ts', sort)).toEqual(['no-restricted-syntax']);
    expect(await lint('packages/transport/src/a.ts', sort)).toEqual(['no-restricted-syntax']);
    expect(await lint('packages/ui/src/surface/atlas.ts', sort)).toEqual([]);
  });

  it('applies the webview rules to the entry of the phone app', async () => {
    const code =
      'export const s = (a: number[], el: HTMLElement) => { el.innerHTML = ""; setTimeout(() => {}, 1); return a.sort(); };';
    expect(await lint('apps/web/src/main.ts', code)).toEqual([
      'neohar/no-inner-html',
      'no-restricted-globals',
      'no-restricted-syntax',
    ]);
  });

  it('does not apply the webview rules outside the webview sources', async () => {
    const code =
      'export const s = (a: number[]) => { setInterval(() => {}, 1); return a.sort(); };';
    expect(await lint('tools/eslint-plugin-neohar/src/a.ts', code)).toEqual([]);
    expect(await lint('scripts/a.ts', code)).toEqual([]);
  });
});
