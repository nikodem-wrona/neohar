// Sends the answer of a card to the core (`interaction.answer`). The card shows what the
// projection says afterwards, and a toast says when the answer arrived. This module only reports
// an error to the card that sent it.

import type { Decision, InteractionValue, Transport } from '@neohar/transport';
import type { CardHost } from './host';

/** What went wrong, in one sentence for the card, or `null` when the core took the answer. */
export async function sendAnswer(
  host: Pick<CardHost, 'transport' | 'via' | 'answered'>,
  interaction: InteractionValue,
  decision: Decision,
): Promise<string | null> {
  try {
    await host.transport.call('interaction.answer', {
      interaction: interaction.id,
      decision,
      via: host.via,
    });
    host.answered(interaction.id);
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

export async function sendDismiss(
  transport: Transport,
  interaction: InteractionValue,
): Promise<string | null> {
  try {
    await transport.call('interaction.dismiss', { interaction: interaction.id });
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}
