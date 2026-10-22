// The account of a phone on the listener of the Mac: who the phone is, the
// pairing with a code, and the removal of the phone. These are plain HTTP routes, apart from the
// JSON-RPC of `/rpc`. The cookie of the phone is the credential. The listener sets it, the page
// never reads it, and the browser sends it with each request to the origin of the page.

import { RpcError } from './errors';

/** A paired phone, as `GET /api/me` and `POST /api/pair` give it. */
export interface PhoneDevice {
  device: string;
  name: string;
}

/** What the account needs of `fetch`. */
export type Fetch = (path: string, init?: RequestInit) => Promise<Response>;

/** The text of a refusal, from the body `{error}` of the listener. */
async function refusal(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { error?: unknown };
    if (typeof body.error === 'string') return body.error;
  } catch {
    // A body that is not JSON: the status says enough.
  }
  return `The Mac refused the request (HTTP ${response.status}).`;
}

export class PhoneAccount {
  constructor(private readonly request: Fetch = (path, init) => fetch(path, init)) {}

  /** The paired phone, or `null` when the listener does not know the cookie. */
  async me(): Promise<PhoneDevice | null> {
    const response = await this.request('/api/me', { credentials: 'same-origin' });
    if (response.status === 401) return null;
    if (!response.ok) throw new RpcError('transport_failed', await refusal(response));
    return (await response.json()) as PhoneDevice;
  }

  /**
   * Pairs the phone with the code of the QR code or the short code. The listener sets the cookie.
   * A refusal has the same text for each cause, so that a guess learns nothing.
   */
  async pair(code: string, name = ''): Promise<PhoneDevice> {
    const response = await this.request('/api/pair', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ code, name }),
    });
    if (!response.ok) throw new RpcError('permission_denied', await refusal(response));
    return (await response.json()) as PhoneDevice;
  }

  /**
   * Puts the bytes of a file that the person attaches into the blob store of the Mac
   * (`POST /blob`), or of another machine of the Mac. The answer is the
   * hash, for `pane.drop_blobs`.
   */
  async upload(file: Blob, name: string, machine?: string): Promise<string> {
    const on = machine ? `&machine=${encodeURIComponent(machine)}` : '';
    const response = await this.request(`/blob?name=${encodeURIComponent(name)}${on}`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/octet-stream' },
      body: file,
    });
    if (!response.ok) throw new RpcError('transport_failed', await refusal(response));
    const body = (await response.json()) as { hash?: unknown };
    if (typeof body.hash !== 'string') throw new RpcError('transport_failed', 'no hash');
    return body.hash;
  }

  /** Removes this phone from the Mac. The listener clears the cookie. */
  async unpair(): Promise<void> {
    const response = await this.request('/api/unpair', {
      method: 'POST',
      credentials: 'same-origin',
    });
    if (!response.ok && response.status !== 401) {
      throw new RpcError('transport_failed', await refusal(response));
    }
  }
}
