import { describe, expect, it } from 'bun:test';
import { RpcError } from '@neohar/transport';
import { requestClose, type CloseTarget } from './close';

const tab: CloseTarget = { kind: 'tab', id: 'tab-1', name: 'Tab 1' };
const workspace: CloseTarget = { kind: 'workspace', id: 'ws-1', name: 'Main' };
const pane: CloseTarget = { kind: 'pane', id: 'pane-1', name: 'w1:p1' };

/** A transport that records the calls and answers with `answer`. */
function fake(answer: (method: string, params: unknown) => Promise<unknown>) {
  const calls: [string, unknown][] = [];
  return {
    calls,
    transport: {
      call: ((method: string, params: unknown) => {
        calls.push([method, params]);
        return answer(method, params);
      }) as never,
    },
  };
}

describe('closing a pane, a tab or a workspace', () => {
  it('sends the close without force and gives nothing when the core closed it', async () => {
    const { calls, transport } = fake(() => Promise.resolve({}));
    expect(await requestClose(transport, tab)).toBeNull();
    expect(await requestClose(transport, workspace)).toBeNull();
    expect(await requestClose(transport, pane)).toBeNull();
    expect(calls).toEqual([
      ['tab.close', { tab: 'tab-1', force: false }],
      ['workspace.close', { workspace: 'ws-1', force: false }],
      ['pane.close', { pane: 'pane-1', force: false }],
    ]);
  });

  it('gives the question when a program runs, with the agent flag of the core', async () => {
    const { transport } = fake(() =>
      Promise.reject(
        new RpcError('confirm_required', 'a program runs', { process: null, is_agent: true }),
      ),
    );
    expect(await requestClose(transport, workspace)).toEqual({ ...workspace, agent: true });
  });

  it('sends force when the person confirms, and shows a failure instead of asking again', async () => {
    const { calls, transport } = fake(() =>
      Promise.reject(new RpcError('confirm_required', 'a program runs', {})),
    );
    const closing = await requestClose(transport, tab, true);
    expect(calls).toEqual([['tab.close', { tab: 'tab-1', force: true }]]);
    expect(closing).toEqual({ ...tab, agent: false, failure: 'a program runs' });
  });

  it('shows the message of another failure', async () => {
    const { transport } = fake(() => Promise.reject(new RpcError('not_found', 'no tab tab-1')));
    expect(await requestClose(transport, tab)).toEqual({
      ...tab,
      agent: false,
      failure: 'no tab tab-1',
    });
  });
});
