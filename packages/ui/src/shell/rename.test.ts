import { describe, expect, it } from 'bun:test';
import { RpcError } from '@neohar/transport';
import { requestRename, sameTarget, type RenameTarget } from './rename';

const tab: RenameTarget = { kind: 'tab', id: 'tab-1', place: 'tabbar' };
const workspace: RenameTarget = { kind: 'workspace', id: 'ws-1', place: 'left' };

/** A transport that records the calls and answers with `answer`. */
function fake(answer: () => Promise<unknown> = () => Promise.resolve({})) {
  const calls: [string, unknown][] = [];
  return {
    calls,
    transport: {
      call: ((method: string, params: unknown) => {
        calls.push([method, params]);
        return answer();
      }) as never,
    },
  };
}

describe('renaming a workspace or a tab', () => {
  it('sends the trimmed name with the method of the kind', async () => {
    const { calls, transport } = fake();
    expect(await requestRename(transport, tab, '  Logs ', 'Tab 1')).toBeNull();
    expect(await requestRename(transport, workspace, 'stonks', 'Workspace 2')).toBeNull();
    expect(calls).toEqual([
      ['tab.rename', { tab: 'tab-1', name: 'Logs' }],
      ['workspace.rename', { workspace: 'ws-1', name: 'stonks' }],
    ]);
  });

  it('sends nothing for an empty text or the old name', async () => {
    const { calls, transport } = fake();
    expect(await requestRename(transport, tab, '   ', 'Tab 1')).toBeNull();
    expect(await requestRename(transport, tab, ' Tab 1 ', 'Tab 1')).toBeNull();
    expect(calls).toEqual([]);
  });

  it('gives the message of a failure', async () => {
    const { transport } = fake(() =>
      Promise.reject(new RpcError('invalid_params', 'a name has at most 100 characters')),
    );
    expect(await requestRename(transport, workspace, 'x', 'y')).toBe(
      'a name has at most 100 characters',
    );
    const broken = fake(() => Promise.reject(new Error('closed')));
    expect(await requestRename(broken.transport, tab, 'x', 'y')).toBe('The tab kept its name.');
  });

  it('knows a field by its kind, its id and its place', () => {
    expect(sameTarget(tab, { ...tab })).toBe(true);
    expect(sameTarget(tab, { ...tab, place: 'left' })).toBe(false);
    expect(sameTarget(tab, null)).toBe(false);
    expect(sameTarget(null, null)).toBe(false);
  });
});
