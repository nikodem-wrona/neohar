// @ts-check
import { describe, it } from 'bun:test';
import { RuleTester } from 'eslint';
import { noInnerHtml } from './no-inner-html.js';

RuleTester.describe = describe;
RuleTester.it = it;

const ruleTester = new RuleTester({
  languageOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
    parserOptions: { ecmaFeatures: { jsx: true } },
  },
});

const forbidden = (/** @type {string} */ name) => ({ messageId: 'forbidden', data: { name } });

ruleTester.run('neohar/no-inner-html', noInnerHtml, {
  valid: [
    'el.textContent = text;',
    'el.append(document.createTextNode(text));',
    'const { value } = el;',
    'const x = { innerText: 1 };',
    'el[name] = html;',
    'const a = <div textContent={text} />;',
    'class A { innerHTMLCache = 1 }',
  ],
  invalid: [
    { code: 'el.innerHTML = html;', errors: [forbidden('innerHTML')] },
    { code: 'const html = el.outerHTML;', errors: [forbidden('outerHTML')] },
    { code: "el['innerHTML'] = html;", errors: [forbidden('innerHTML')] },
    {
      code: "el.insertAdjacentHTML('beforeend', html);",
      errors: [forbidden('insertAdjacentHTML')],
    },
    { code: 'Object.assign(el, { innerHTML: html });', errors: [forbidden('innerHTML')] },
    { code: 'const { innerHTML } = el;', errors: [forbidden('innerHTML')] },
    { code: 'const a = <div innerHTML={html} />;', errors: [forbidden('innerHTML')] },
    { code: 'const a = <div prop:innerHTML={html} />;', errors: [forbidden('innerHTML')] },
    { code: 'const a = <div {...{ outerHTML: html }} />;', errors: [forbidden('outerHTML')] },
  ],
});
