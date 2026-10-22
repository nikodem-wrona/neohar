// Closing a pane, a tab or a workspace from the chrome. The first call goes without
// `force`. When a program runs in a pane, the core answers `confirm_required`, and the window asks
// the person. The answer calls again with `force`, and the core kills every process of the panes.

import { RpcError, type Transport } from '@neohar/transport';

/** What closes: a pane, a tab or a workspace, with its id and its name. */
export interface CloseTarget {
  kind: 'pane' | 'tab' | 'workspace';
  id: string;
  name: string;
}

/** A close that waits for the person: the question, or the failure of the last call. */
export interface Closing extends CloseTarget {
  /** An agent runs in one of the panes. */
  agent: boolean;
  /** The last call failed with this message. */
  failure?: string;
}

/** Whether the details of `confirm_required` say that an agent runs. */
function agentOf(details: unknown): boolean {
  return (
    typeof details === 'object' &&
    details !== null &&
    'is_agent' in details &&
    (details as { is_agent: unknown }).is_agent === true
  );
}

/**
 * Asks the core to close the target. Gives `null` when it closed, and else what the window shows:
 * the question of `confirm_required`, or the message of a failure.
 */
export async function requestClose(
  transport: Pick<Transport, 'call'>,
  target: CloseTarget,
  force = false,
): Promise<Closing | null> {
  try {
    if (target.kind === 'pane') await transport.call('pane.close', { pane: target.id, force });
    else if (target.kind === 'tab') await transport.call('tab.close', { tab: target.id, force });
    else await transport.call('workspace.close', { workspace: target.id, force });
    return null;
  } catch (failure: unknown) {
    if (!force && failure instanceof RpcError && failure.code === 'confirm_required') {
      return { ...target, agent: agentOf(failure.details) };
    }
    return {
      ...target,
      agent: false,
      failure: failure instanceof RpcError ? failure.message : `The ${target.kind} did not close.`,
    };
  }
}
