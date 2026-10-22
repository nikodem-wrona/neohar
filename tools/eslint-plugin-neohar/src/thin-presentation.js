// @ts-check
// The thin-presentation rules for the webview sources.
// `eslint.config.js` spreads `thinPresentation(plugin)`; the tests import the same pieces.

/** The webview sources: the UI packages and the thin app entries of the desk and of the phone. */
export const WEBVIEW_SOURCES = [
  'packages/*/src/**/*.{ts,tsx,js,jsx}',
  'apps/desktop/src/**/*.{ts,tsx,js,jsx}',
  'apps/web/src/**/*.{ts,tsx,js,jsx}',
];
export const TEST_FILES = ['**/*.test.{ts,tsx,js,jsx}'];
export const UI_SOURCES = ['packages/ui/src/**/*.{ts,tsx,js,jsx}'];
export const UI_TESTS = ['packages/ui/src/**/*.test.{ts,tsx,js,jsx}'];
export const RICH_HTML_FILE = 'packages/ui/src/rich/RichHtml.tsx';
export const TIMING_FILE = 'packages/ui/src/lib/timing.ts';
export const SURFACE_SOURCES = ['packages/ui/src/surface/**'];

/**
 * Import specifiers `packages/ui` may use. Everything else is an error.
 * Each entry is a regular expression that must match the whole specifier.
 */
const ALLOWED_IMPORTS = [
  'solid-js(?:/.*)?',
  '@neohar/transport',
  '@neohar/proto-wasm',
  '\\.{1,2}(?:/.*)?',
];

/**
 * The options of `no-restricted-imports` that reject every specifier except the allowed ones.
 * @param {string[]} [extraAllowed] more specifiers, as whole-match regular expressions
 */
export function importOptions(extraAllowed = []) {
  const allowed = [...ALLOWED_IMPORTS, ...extraAllowed].join('|');
  return {
    patterns: [
      {
        regex: `^(?!(?:${allowed})$)`,
        message:
          'packages/ui may import only solid-js, @neohar/transport, @neohar/proto-wasm and relative modules.',
      },
    ],
  };
}

const NO_POLLING = 'Everything is push. Do not poll with timers.';
const NO_TIMER = 'Timers live in packages/ui/src/lib/timing.ts only.';

/**
 * Options of `no-restricted-globals` for the given timer functions.
 * @param {string[]} names
 */
export function timerGlobals(names) {
  return names.map((name) => ({
    name,
    message: name === 'setInterval' ? NO_POLLING : NO_TIMER,
  }));
}

/**
 * Options of `no-restricted-properties`: the same timers reached through a global object.
 * @param {string[]} names
 */
export function timerProperties(names) {
  return ['window', 'globalThis', 'self'].flatMap((object) =>
    names.map((property) => ({
      object,
      property,
      message: property === 'setInterval' ? NO_POLLING : NO_TIMER,
    })),
  );
}

const FORMATTING = 'The core formats and sorts. The webview paints what it receives.';
const METHODS =
  /^(sort|toSorted|localeCompare|toLocaleString|toLocaleDateString|toLocaleTimeString)$/;

/** Selectors for `no-restricted-syntax`: sorting, locale formatting and `Intl`. */
export const SYNTAX_SELECTORS = [
  {
    selector: `CallExpression > MemberExpression.callee[computed=false][property.name=${METHODS}]`,
    message: FORMATTING,
  },
  {
    selector: `CallExpression > MemberExpression.callee[computed=true][property.value=${METHODS}]`,
    message: FORMATTING,
  },
  { selector: "MemberExpression[object.name='Intl']", message: FORMATTING },
  { selector: "MemberExpression[property.name='Intl']", message: FORMATTING },
];

/**
 * The flat-config blocks that enforce the thin-presentation rules.
 * @param {object} plugin the `neohar` plugin object
 * @returns {import('eslint').Linter.Config[]}
 */
export function thinPresentation(plugin) {
  return [
    {
      name: 'neohar/inner-html',
      files: WEBVIEW_SOURCES,
      plugins: { neohar: plugin },
      rules: { 'neohar/no-inner-html': 'error' },
    },
    {
      name: 'neohar/inner-html-allowed',
      files: [RICH_HTML_FILE],
      rules: { 'neohar/no-inner-html': 'off' },
    },
    {
      name: 'neohar/ui-imports',
      files: UI_SOURCES,
      rules: { 'no-restricted-imports': ['error', importOptions()] },
    },
    {
      name: 'neohar/ui-imports-in-tests',
      files: UI_TESTS,
      rules: { 'no-restricted-imports': ['error', importOptions(['bun:test'])] },
    },
    {
      name: 'neohar/no-polling',
      files: WEBVIEW_SOURCES,
      rules: {
        'no-restricted-globals': ['error', ...timerGlobals(['setInterval'])],
        'no-restricted-properties': ['error', ...timerProperties(['setInterval'])],
      },
    },
    {
      name: 'neohar/no-timers',
      files: WEBVIEW_SOURCES,
      ignores: [TIMING_FILE, ...TEST_FILES],
      rules: {
        'no-restricted-globals': ['error', ...timerGlobals(['setInterval', 'setTimeout'])],
        'no-restricted-properties': ['error', ...timerProperties(['setInterval', 'setTimeout'])],
      },
    },
    {
      name: 'neohar/no-formatting',
      files: WEBVIEW_SOURCES,
      ignores: [...SURFACE_SOURCES, ...TEST_FILES],
      rules: { 'no-restricted-syntax': ['error', ...SYNTAX_SELECTORS] },
    },
  ];
}
