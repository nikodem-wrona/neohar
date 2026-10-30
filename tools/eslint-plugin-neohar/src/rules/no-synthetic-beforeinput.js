// @ts-check

/**
 * Forbids the JSX attribute `onBeforeInput`. React makes that event from `keypress` and
 * `textInput`: it has no `inputType`, and it does not come for `deleteContentBackward`. The typing
 * of a phone needs the native `beforeinput`, so a component listens with `addEventListener`.
 *
 * @type {import('eslint').Rule.RuleModule}
 */
export const noSyntheticBeforeInput = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Forbid the synthetic beforeinput event of React.',
    },
    schema: [],
    messages: {
      forbidden:
        "`onBeforeInput` of React is not the native `beforeinput`. Use `addEventListener('beforeinput', …)`.",
    },
  },
  create(context) {
    return {
      JSXAttribute(/** @type {any} */ node) {
        const name = node.name.type === 'JSXNamespacedName' ? node.name.name : node.name;
        if (name.name === 'onBeforeInput' || name.name === 'onBeforeInputCapture') {
          context.report({ node, messageId: 'forbidden' });
        }
      },
    };
  },
};
