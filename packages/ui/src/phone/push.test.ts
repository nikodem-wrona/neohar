import { describe, expect, test } from 'bun:test';
import type { Transport } from '@neohar/transport';
import {
  DENIED,
  keyBytes,
  NO_KEY,
  NOT_SUPPORTED,
  type PushBrowser,
  PushProblem,
  type SubscriptionKeys,
  turnOff,
  turnOn,
} from './push';

/** The VAPID key of RFC 8291, section 5: an uncompressed P-256 point. */
const KEY =
  'BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8';

const KEYS: SubscriptionKeys = {
  endpoint: 'https://web.push.apple.com/QGhP9m2Aw',
  p256dh: 'BCVx',
  auth: 'BTBZ',
};

/** A browser that records what the switch asks of it. */
function browser(change: { supported?: boolean; permission?: NotificationPermission } = {}) {
  const asked: string[] = [];
  let key: Uint8Array | null = null;
  const fake: PushBrowser = {
    supported: () => change.supported ?? true,
    requestPermission: () => {
      asked.push('permission');
      return Promise.resolve(change.permission ?? 'granted');
    },
    subscribe: (applicationServerKey) => {
      asked.push('subscribe');
      key = applicationServerKey;
      return Promise.resolve(KEYS);
    },
    unsubscribe: () => {
      asked.push('unsubscribe');
      return Promise.resolve();
    },
  };
  return { fake, asked, key: () => key };
}

/** A transport that records the calls. */
function transport() {
  const calls: { method: string; params: unknown }[] = [];
  const fake = {
    call: (method: string, params: unknown) => {
      calls.push({ method, params });
      return Promise.resolve({});
    },
  } as unknown as Transport;
  return { fake, calls };
}

describe('the switch Notifications', () => {
  test('asks for the permission, subscribes with the key of the core and sends the subscription', async () => {
    const b = browser();
    const t = transport();
    await turnOn(b.fake, t.fake, KEY);
    expect(b.asked).toEqual(['permission', 'subscribe']);
    expect(b.key()?.length).toBe(65);
    expect(b.key()?.[0]).toBe(4);
    expect(t.calls).toEqual([{ method: 'phone.push_subscribe', params: KEYS }]);
  });

  test('a page that is not an app of the Home Screen says how to add it', async () => {
    const b = browser({ supported: false });
    const t = transport();
    await expect(turnOn(b.fake, t.fake, KEY)).rejects.toThrow(new PushProblem(NOT_SUPPORTED));
    expect(b.asked).toEqual([]);
    expect(t.calls).toEqual([]);
  });

  test('a refused permission sends nothing', async () => {
    const b = browser({ permission: 'denied' });
    const t = transport();
    await expect(turnOn(b.fake, t.fake, KEY)).rejects.toThrow(DENIED);
    expect(b.asked).toEqual(['permission']);
    expect(t.calls).toEqual([]);
  });

  test('no key of the core asks for nothing', async () => {
    const b = browser();
    const t = transport();
    await expect(turnOn(b.fake, t.fake, undefined)).rejects.toThrow(NO_KEY);
    expect(b.asked).toEqual([]);
  });

  test('off ends the subscription in the browser and in the core', async () => {
    const b = browser();
    const t = transport();
    await turnOff(b.fake, t.fake);
    expect(b.asked).toEqual(['unsubscribe']);
    expect(t.calls).toEqual([{ method: 'phone.push_unsubscribe', params: {} }]);
  });
});

describe('a key in base64url', () => {
  test('gives its bytes, with or without padding', () => {
    expect([...keyBytes('BTBZMqHH6r4Tts7J_aSIgg')]).toEqual([
      0x05, 0x30, 0x59, 0x32, 0xa1, 0xc7, 0xea, 0xbe, 0x13, 0xb6, 0xce, 0xc9, 0xfd, 0xa4, 0x88,
      0x82,
    ]);
    expect([...keyBytes('BTBZMqHH6r4Tts7J_aSIgg==')]).toEqual([
      ...keyBytes('BTBZMqHH6r4Tts7J_aSIgg'),
    ]);
    expect(keyBytes(KEY).length).toBe(65);
  });
});
