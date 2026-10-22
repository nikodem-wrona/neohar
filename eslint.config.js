import eslint from '@eslint/js';
import neohar, { thinPresentationConfigs } from '@neohar/eslint-plugin';
import tseslint from 'typescript-eslint';

/** The webview sources that the rules with type information check. */
const TYPED_SOURCES = [
  'packages/ui/src/**/*.{ts,tsx}',
  'packages/transport/src/**/*.{ts,tsx}',
  'apps/*/src/**/*.{ts,tsx}',
];

export default [
  {
    ignores: [
      'target/**',
      '**/node_modules/**',
      '**/dist/**',
      'packages/proto-wasm/pkg/**',
      'website/build/**',
      'website/.docusaurus/**',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  { plugins: { neohar } },
  ...thinPresentationConfigs,
  // A promise that nobody awaits or catches loses its error, and `void` does not catch it. Each
  // package has its own tsconfig, and the project service finds it for each file.
  {
    files: TYPED_SOURCES,
    ignores: ['**/*.typetest.ts'],
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    rules: {
      '@typescript-eslint/no-floating-promises': [
        'error',
        {
          ignoreVoid: false,
          // The input of a pane: these promises do not reject.
          allowForKnownSafeCalls: [
            {
              from: 'file',
              name: ['send', 'paste', 'mouse', 'resend'],
              path: 'src/input/sender.ts',
            },
          ],
        },
      ],
    },
  },
];
