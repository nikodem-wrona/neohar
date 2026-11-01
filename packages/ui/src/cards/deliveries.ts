// The answers that the cards of this window sent, until the core says how each one ended. Then a
// toast in the corner says it: the answer arrived, or it may not have. The card itself says
// nothing about an answer that arrived. The toast shows also when the Inbox is closed.

import { harnessName } from '../agents/badge';
import type { Toasts } from '../app/toasts';
import type { Stores } from '../stores';
import { answerArrived, answerToast } from './model';

export interface Deliveries {
  /** A card of this window sent an answer to this interaction, and the core took it. */
  expect(interaction: string): void;
  /** Stops to follow the interactions. */
  stop(): void;
}

/** The parts of the stores that the deliveries read. */
export interface DeliveryStores {
  agents: Pick<Stores['agents'], 'get'>;
  interactions: Pick<Stores['interactions'], 'get' | 'subscribe'>;
}

export function watchDeliveries(stores: DeliveryStores, toasts: Toasts): Deliveries {
  let waiting: readonly string[] = [];
  const harnessOf = (run: string | null | undefined) => {
    const agent = run ? stores.agents.get().byKey[run] : undefined;
    return agent ? harnessName(agent.harness) : 'the agent';
  };
  // Shows a toast for each answer whose interaction ended.
  const check = (): void => {
    const interactions = stores.interactions.get().byKey;
    const ended = waiting.filter((id) => answerArrived(interactions[id]));
    if (ended.length === 0) return;
    waiting = waiting.filter((id) => !ended.includes(id));
    for (const id of ended) {
      const interaction = interactions[id];
      const toast = interaction ? answerToast(interaction, harnessOf(interaction.run)) : null;
      if (toast) toasts.show(toast);
    }
  };
  const stop = stores.interactions.subscribe(check);

  return {
    expect: (interaction) => {
      if (!waiting.includes(interaction)) waiting = [...waiting, interaction];
      check();
    },
    stop,
  };
}
