// The error of a method. The core sends `{code, message, details}`. The UI acts on
// `code` for the few cases that need it, and it shows `message`.

import type { ErrorCode } from './generated';

export class RpcError extends Error {
  /** A code of the contract, or a code of the transport (`ws_closed`, `ws_failed`). */
  readonly code: ErrorCode | 'ws_closed' | 'ws_failed' | 'transport_failed';
  readonly details: unknown;

  constructor(code: RpcError['code'], message: string, details?: unknown) {
    super(message);
    this.name = 'RpcError';
    this.code = code;
    this.details = details;
  }
}

/**
 * Turns what a transport rejects with into an `RpcError`. Tauri rejects with the object that the
 * host serialized (`{code, message, details}`). A JSON-RPC reply has the code in `error.data`.
 * Anything else becomes `transport_failed`.
 */
export function toRpcError(value: unknown): RpcError {
  if (value instanceof RpcError) return value;
  if (typeof value === 'object' && value !== null) {
    const object = value as Record<string, unknown>;
    // The envelope of JSON-RPC: `{code: -32000, message, data: {code, details}}`.
    const data = object['data'];
    if (typeof data === 'object' && data !== null) {
      const inner = data as Record<string, unknown>;
      if (typeof inner['code'] === 'string' && typeof object['message'] === 'string') {
        return new RpcError(inner['code'] as RpcError['code'], object['message'], inner['details']);
      }
    }
    if (typeof object['code'] === 'string' && typeof object['message'] === 'string') {
      return new RpcError(object['code'] as RpcError['code'], object['message'], object['details']);
    }
  }
  const message = value instanceof Error ? value.message : String(value);
  return new RpcError('transport_failed', message);
}
