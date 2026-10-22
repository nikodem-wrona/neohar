import { expect, test } from 'bun:test';
import { RpcError, toRpcError } from './errors';

test('an object with a code and a message from the host becomes an RpcError', () => {
  const error = toRpcError({ code: 'not_found', message: 'no pane w1:p9', details: { x: 1 } });
  expect(error).toBeInstanceOf(RpcError);
  expect(error.code).toBe('not_found');
  expect(error.message).toBe('no pane w1:p9');
  expect(error.details).toEqual({ x: 1 });
});

test('a reply of JSON-RPC has the code of the contract in data', () => {
  const error = toRpcError({
    code: -32000,
    message: 'the pane runs',
    data: { code: 'confirm_required', details: { process: 'vim', is_agent: false } },
  });
  expect(error.code).toBe('confirm_required');
  expect(error.details).toEqual({ process: 'vim', is_agent: false });
});

test('anything else is a failure of the transport', () => {
  expect(toRpcError('boom').code).toBe('transport_failed');
  expect(toRpcError('boom').message).toBe('boom');
  expect(toRpcError(new Error('closed')).message).toBe('closed');
  expect(toRpcError(null).code).toBe('transport_failed');
  const same = new RpcError('busy', 'later');
  expect(toRpcError(same)).toBe(same);
});
