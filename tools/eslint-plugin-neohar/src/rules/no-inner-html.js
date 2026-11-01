// @ts-check

/** DOM properties and methods that parse a string as HTML. */
const FORBIDDEN = new Set([
  'innerHTML',
  'outerHTML',
  'insertAdjacentHTML',
  'dangerouslySetInnerHTML',
]);

/**
 * The name a key or property expression spells out, or null when it is computed at run time.
 * @param {import('estree').Node} node
 * @param {boolean} computed
 * @returns {string | null}
 */
function staticName(node, computed) {
  if (!computed && node.type === 'Identifier') return node.name;
  if (node.type === 'Literal' && typeof node.value === 'string') return node.value;
  return null;
}

/**
 * Forbids every way to put a string into the DOM as HTML: `el.innerHTML`, `el["outerHTML"]`,
 * `el.insertAdjacentHTML(...)`, object keys such as `{ innerHTML: x }` (also in a destructuring
 * pattern or a spread into a JSX element), the JSX attribute `dangerouslySetInnerHTML` of React and
 * the JSX attributes `innerHTML`, `outerHTML` and `prop:innerHTML`. The one allowed place,
 * `RichHtml.tsx`, turns the rule off in `eslint.config.js`.
 *
 * @type {import('eslint').Rule.RuleModule}
 */
export const noInnerHtml = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Forbid HTML injection outside RichHtml.tsx.',
    },
    schema: [],
    messages: {
      forbidden:
        '`{{name}}` parses a string as HTML. Only packages/ui/src/rich/RichHtml.tsx may do that.',
    },
  },
  create(context) {
    /**
     * @param {import('estree').Node | import('estree-jsx').JSXIdentifier} node
     * @param {string | null} name
     */
    function check(node, name) {
      if (name !== null && FORBIDDEN.has(name)) {
        context.report({ node: /** @type {any} */ (node), messageId: 'forbidden', data: { name } });
      }
    }
    return {
      MemberExpression(node) {
        check(node.property, staticName(node.property, node.computed));
      },
      Property(node) {
        check(node.key, staticName(node.key, node.computed));
      },
      JSXAttribute(/** @type {any} */ node) {
        const name = node.name.type === 'JSXNamespacedName' ? node.name.name : node.name;
        check(node.name, name.name);
      },
    };
  },
};
