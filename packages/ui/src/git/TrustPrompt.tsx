// The prompt of a repository that the core runs no git command in: the Changes tab of the drawer
// and the Git view show it in place of their content. The core decides when to ask (`trust` of
// `changes_head:<window>` and `git_head:<window>`): the person never trusted the repository, or a
// setting that names a program changed after the trust. **Trust** calls `git.trust` with the path
// of the core, and the core then reads the repository and sends the rows. The page shows nothing
// before the patch comes.

import type { GitTrustAsk } from '@neohar/transport';
import { useApp } from '../app/context';
import { trustPrompt } from './model';

export function TrustPrompt(props: { ask: GitTrustAsk; className?: string }) {
  const { run } = useApp();
  const text = trustPrompt(props.ask);
  return (
    <section
      className={`trust-prompt ${props.className ?? ''}`}
      aria-label="Trust this repository"
      data-testid="trust-prompt"
    >
      <h3 className="trust-title">{text.title}</h3>
      <p className="trust-path" title={props.ask.path}>
        {props.ask.path}
      </p>
      {text.lines.map((line, index) => (
        <p key={index} className="trust-text">
          {line}
        </p>
      ))}
      {text.changed ? <p className="trust-changed">{text.changed}</p> : null}
      <button
        type="button"
        className="trust-button"
        onClick={() => run('git.trust', { path: props.ask.path })}
      >
        Trust
      </button>
    </section>
  );
}
