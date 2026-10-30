// The answers that the cards of this window sent, until the core says how each one ended. Then a
// toast in the corner says it: the answer arrived, or it may not have. The card itself says
// nothing about an answer that arrived. The toast shows also when the Inbox is closed.

import { createEffect, createSignal, untrack } from 'solid-js';
import { harnessName } from '../agents/badge';
import type { Toasts } from '../app/toasts';
import type { Stores } from '../stores';
import { answerArrived, answerToast } from './model';

export interface Deliveries {
  /** A card of this window sent an answer to this interaction, and the core took it. */
  expect(interaction: string): void;
}

export function watchDeliveries(stores: Stores, toasts: Toasts): Deliveries {
  const [waiting, setWaiting] = createSignal<readonly string[]>([]);
  const harnessOf = (run: string | null | undefined) => {
    const agent = run ? stores.agents.state.byKey[run] : undefined;
    return agent ? harnessName(agent.harness) : 'the agent';
  };

  createEffect(() => {
    const ended = waiting().filter((id) => answerArrived(stores.interactions.state.byKey[id]));
    if (ended.length === 0) return;
    untrack(() => {
      for (const id of ended) {
        const interaction = stores.interactions.state.byKey[id];
        const toast = interaction ? answerToast(interaction, harnessOf(interaction.run)) : null;
        if (toast) toasts.show(toast);
      }
      setWaiting((all) => all.filter((id) => !ended.includes(id)));
    });
  });

  return {
    expect: (interaction) => {
      setWaiting((all) => (all.includes(interaction) ? all : [...all, interaction]));
    },
  };
}
