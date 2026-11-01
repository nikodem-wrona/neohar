// @ts-check
import { describe, it } from 'bun:test';
import { RuleTester } from 'eslint';
import { noSyntheticBeforeInput } from './no-synthetic-beforeinput.js';

RuleTester.describe = describe;
RuleTester.it = it;

const ruleTester = new RuleTester({
  languageOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
    parserOptions: { ecmaFeatures: { jsx: true } },
  },
});

ruleTester.run('neohar/no-synthetic-beforeinput', noSyntheticBeforeInput, {
  valid: [
    'const a = <textarea onInput={f} />;',
    "field.addEventListener('beforeinput', f);",
    'const a = { onBeforeInput: f };',
  ],
  invalid: [
    { code: 'const a = <textarea onBeforeInput={f} />;', errors: [{ messageId: 'forbidden' }] },
    {
      code: 'const a = <div onBeforeInputCapture={f} />;',
      errors: [{ messageId: 'forbidden' }],
    },
  ],
});
