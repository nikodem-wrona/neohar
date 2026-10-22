// The inputs that the core may not have.
//
// A `pane.input` that fails, or that the holder did not confirm, can be lost with a connection.
// The holder drops an `input_id` that it saw, so the client can send an input again with the same
// id and it is written once. The outbox keeps each input until a frame says that the holder wrote
// it, or until 30 seconds have passed, and it sends the kept inputs again after a reconnect.

import type { Params } from './generated';

/** How long an input stays in the outbox without a confirmation. */
export const OUTBOX_TTL_MS = 30_000;

export type InputParams = Params<'pane.input'>;

export class InputOutbox {
  private readonly pending = new Map<number, { params: InputParams; sentAt: number }>();

  constructor(
    private readonly send: (params: InputParams) => Promise<unknown>,
    private readonly now: () => number = Date.now,
  ) {}

  /** The number of inputs that wait for a confirmation. */
  get size(): number {
    this.expire();
    return this.pending.size;
  }

  /** Sends an input and keeps it. A failed send stays in the outbox for a later `resend`. */
  async submit(params: InputParams): Promise<void> {
    this.pending.set(params.input_id, { params, sentAt: this.now() });
    try {
      await this.send(params);
    } catch {
      // The input stays. It goes out again with the same id after a reconnect.
    }
  }

  /** A frame confirmed these inputs. */
  confirmed(inputIds: Iterable<number>): void {
    for (const id of inputIds) this.pending.delete(id);
  }

  /** Sends the kept inputs again, oldest first, after the connection is back. */
  async resend(): Promise<void> {
    this.expire();
    const kept = [...this.pending.values()];
    for (const entry of kept) {
      try {
        await this.send(entry.params);
      } catch {
        // Still not there. The next `resend` tries again, while the input is young.
        return;
      }
    }
  }

  private expire(): void {
    const limit = this.now() - OUTBOX_TTL_MS;
    for (const [id, entry] of this.pending) {
      if (entry.sentAt < limit) this.pending.delete(id);
    }
  }
}
