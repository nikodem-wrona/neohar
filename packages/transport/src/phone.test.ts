import { expect, test } from 'bun:test';
import { RpcError } from './errors';
import { type Fetch, PhoneAccount } from './phone';

interface Sent {
  path: string;
  init: RequestInit | undefined;
}

/** A listener that answers each request with the next response. */
function listener(...responses: Response[]): { fetch: Fetch; sent: Sent[] } {
  const sent: Sent[] = [];
  return {
    sent,
    fetch: (path, init) => {
      sent.push({ path, init });
      const next = responses.shift();
      return next ? Promise.resolve(next) : Promise.reject(new Error('no answer'));
    },
  };
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

test('me gives the phone, or null when the listener does not know the cookie', async () => {
  const { fetch, sent } = listener(json({ device: 'd1', name: 'iPhone' }), json({}, 401));
  const account = new PhoneAccount(fetch);
  expect(await account.me()).toEqual({ device: 'd1', name: 'iPhone' });
  expect(await account.me()).toBeNull();
  expect(sent.map((s) => [s.path, s.init?.credentials])).toEqual([
    ['/api/me', 'same-origin'],
    ['/api/me', 'same-origin'],
  ]);
});

test('pair posts the code and the name, and a refusal keeps the text of the listener', async () => {
  const refused = 'The code is wrong, or the offer ended. Ask the Mac for a new code.';
  const { fetch, sent } = listener(
    json({ device: 'd1', name: 'Phone' }),
    json({ error: refused }, 403),
  );
  const account = new PhoneAccount(fetch);
  expect(await account.pair('ABCD2345')).toEqual({ device: 'd1', name: 'Phone' });
  expect(sent[0]?.init?.method).toBe('POST');
  expect(JSON.parse(String(sent[0]?.init?.body))).toEqual({ code: 'ABCD2345', name: '' });
  const failure = await account.pair('WRONG').catch((error: unknown) => error);
  expect(failure).toBeInstanceOf(RpcError);
  expect((failure as RpcError).message).toBe(refused);
});

test('unpair posts, and a phone that the listener does not know is unpaired already', async () => {
  const { fetch, sent } = listener(
    new Response(null, { status: 204 }),
    json({ error: 'not paired' }, 401),
    new Response('broken', { status: 500 }),
  );
  const account = new PhoneAccount(fetch);
  await account.unpair();
  await account.unpair();
  const failure = await account.unpair().catch((error: unknown) => error);
  expect((failure as RpcError).message).toBe('The Mac refused the request (HTTP 500).');
  expect(sent.map((s) => [s.path, s.init?.method])).toEqual([
    ['/api/unpair', 'POST'],
    ['/api/unpair', 'POST'],
    ['/api/unpair', 'POST'],
  ]);
});

test('upload posts the bytes and names the machine of a desk of another machine', async () => {
  const { fetch, sent } = listener(json({ hash: 'a'.repeat(64) }), json({ hash: 'b'.repeat(64) }));
  const account = new PhoneAccount(fetch);
  const file = new Blob(['png']);
  expect(await account.upload(file, 'shot one.png')).toBe('a'.repeat(64));
  expect(await account.upload(file, 'shot.png', 'studio')).toBe('b'.repeat(64));
  expect(sent.map((s) => [s.path, s.init?.method])).toEqual([
    ['/blob?name=shot%20one.png', 'POST'],
    ['/blob?name=shot.png&machine=studio', 'POST'],
  ]);
});
