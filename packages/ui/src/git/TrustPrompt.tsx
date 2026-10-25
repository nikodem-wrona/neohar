// The prompt of a repository that the core runs no git command in: the Changes tab of the drawer
// and the Git view show it in place of their content. The core decides when to ask (`trust` of
// `changes_head:<window>` and `git_head:<window>`): the person never trusted the repository, or a
// setting that names a program changed after the trust. **Trust** calls `git.trust` with the path
// of the core, and the core then reads the repository and sends the rows. The page shows nothing
// before the patch comes.

import type { GitTrustAsk } from '@neohar/transport';
import { For, Show } from 'solid-js';
import { useApp } from '../app/context';
import { trustPrompt } from './model';

export function TrustPrompt(props: { ask: GitTrustAsk; class?: string }) {
  const { run } = useApp();
  const text = () => trustPrompt(props.ask);
  return (
    <section
      class={`trust-prompt ${props.class ?? ''}`}
      aria-label="Trust this repository"
      data-testid="trust-prompt"
    >
      <h3 class="trust-title">{text().title}</h3>
      <p class="trust-path" title={props.ask.path}>
        {props.ask.path}
      </p>
      <For each={text().lines}>{(line) => <p class="trust-text">{line}</p>}</For>
      <Show when={text().changed}>{(changed) => <p class="trust-changed">{changed()}</p>}</Show>
      <button
        type="button"
        class="trust-button"
        onClick={() => run('git.trust', { path: props.ask.path })}
      >
        Trust
      </button>
    </section>
  );
}
