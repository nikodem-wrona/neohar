import { expect, test } from 'bun:test';
import type { InteractionValue } from '@neohar/transport';
import { createToasts } from '../app/toasts';
import type { Timers } from '../lib/timing';
import { createValue } from '../lib/value';
import { emptyState, type ProjectionState } from '../stores/reducer';
import { watchDeliveries, type DeliveryStores } from './deliveries';

const never: Timers = { set: () => 0, clear: () => undefined };

const interaction = (state: InteractionValue['state']): InteractionValue =>
  ({ id: 'i-1', title: 'Run `ls`', state, run: null }) as unknown as InteractionValue;

function stateOf(value: InteractionValue): ProjectionState<InteractionValue> {
  return { byKey: { [value.id]: value }, order: [value.id], version: 1, status: 'live' };
}

function setup() {
  const interactions = createValue(stateOf(interaction('delivering')));
  const stores = {
    agents: { get: () => emptyState() },
    interactions,
  } as unknown as DeliveryStores;
  const toasts = createToasts(never);
  return { interactions, toasts, deliveries: watchDeliveries(stores, toasts) };
}

test('a toast says that an answer arrived when the core delivered it', () => {
  const { interactions, toasts, deliveries } = setup();
  deliveries.expect('i-1');
  expect(toasts.shown.get()).toEqual([]);
  interactions.set(stateOf(interaction('delivered')));
  expect(toasts.shown.get().map((toast) => toast.title)).toEqual(['Answer delivered to the agent']);
  // The answer is at its end: a later change of the interaction shows no other toast.
  interactions.set(stateOf(interaction('delivered')));
  expect(toasts.shown.get()).toHaveLength(1);
});

test('no toast after a stop, and none for an answer that this window did not send', () => {
  const { interactions, toasts, deliveries } = setup();
  interactions.set(stateOf(interaction('delivered')));
  expect(toasts.shown.get()).toEqual([]);
  deliveries.stop();
  interactions.set(stateOf(interaction('delivering')));
  deliveries.expect('i-1');
  expect(toasts.shown.get()).toEqual([]);
});
