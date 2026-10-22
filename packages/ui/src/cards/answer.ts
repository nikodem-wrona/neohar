// Sends the answer of a card to the core (`interaction.answer`). The card shows what the
// projection says afterwards; this module only reports an error to the card that sent it.

import type { Decision, InteractionValue, Transport, Via } from '@neohar/transport';

/** What went wrong, in one sentence for the card, or `null` when the core took the answer. */
export async function sendAnswer(
  transport: Transport,
  interaction: InteractionValue,
  decision: Decision,
  via: Via,
): Promise<string | null> {
  try {
    await transport.call('interaction.answer', { interaction: interaction.id, decision, via });
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
