// A type-level test of the generated API types. `tsc` checks this file: a line with
// `@ts-expect-error` must not compile, and the others must.

import type { Method, Params, ProjectionName, Result, UiMethod } from './generated';

declare function call<M extends Method>(method: M, params: Params<M>): Promise<Result<M>>;

// A call with the right params compiles, and its result has the type of the method.
void call('pane.split', { pane: 'w3:p5', direction: 'right' });
void call('session.info', {}).then((info) => info.api_version.toUpperCase());

// Wrong params do not compile.
// @ts-expect-error `direction` is missing.
void call('pane.split', { pane: 'w3:p5' });
// @ts-expect-error `sideways` is not a direction.
void call('pane.split', { pane: 'w3:p5', direction: 'sideways' });
// @ts-expect-error no such method.
void call('pane.dance', {});

// The result is typed: a field that does not exist is an error.
void call('pane.show', { pane: 'w3:p5' }).then(
  (shown) =>
    // @ts-expect-error a pane has no `colour`.
    shown.pane.colour,
);

// A decision is a tagged union.
void call('interaction.answer', {
  interaction: 'i42',
  decision: { kind: 'approval', behavior: 'allow', always: true },
  via: 'card',
});
void call('interaction.answer', {
  interaction: 'i42',
  // @ts-expect-error an approval has no `answers`.
  decision: { kind: 'approval', behavior: 'allow', answers: [] },
  via: 'card',
});

// The UI cannot call the methods of a hook shim.
const uiOnly: UiMethod = 'pane.input';
// @ts-expect-error `adapter.signal` is for panes only.
const notForUi: UiMethod = 'adapter.signal';
void [uiOnly, notForUi];

// A projection name takes its parameter.
const names: ProjectionName[] = ['tree', 'timeline:01M4F1W7EM2GN3EV6NHXZJEGGA', 'health'];
// @ts-expect-error `timeline` needs a run.
const bad: ProjectionName = 'timeline';
void [names, bad];
