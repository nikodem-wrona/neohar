// @ts-check
import { noInnerHtml } from './rules/no-inner-html.js';
import { noSyntheticBeforeInput } from './rules/no-synthetic-beforeinput.js';
import { thinPresentation } from './thin-presentation.js';

/** The `neohar` ESLint plugin: custom rules and the thin-presentation configuration. */
const plugin = {
  meta: { name: '@neohar/eslint-plugin' },
  rules: { 'no-inner-html': noInnerHtml, 'no-synthetic-beforeinput': noSyntheticBeforeInput },
};

export default plugin;

/** Flat-config blocks that enforce the thin-presentation rules on the webview sources. */
export const thinPresentationConfigs = thinPresentation(plugin);
