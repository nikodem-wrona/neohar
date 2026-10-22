// The push subscription of the phone. The switch Notifications asks for the permission
// after a tap (iOS needs a gesture), subscribes with the VAPID key of the core and sends the
// subscription to the core. The core keeps it, and the row of the phone in `phones` says whether
// push is on. This module keeps no state: it calls the browser and the core.

import type { Transport } from '@neohar/transport';

/** The parts of a subscription that the core needs. */
export interface SubscriptionKeys {
  endpoint: string;
  p256dh: string;
  auth: string;
}

/** What the switch uses of the browser. The tests give a fake. */
export interface PushBrowser {
  /** Whether the page can subscribe: an app of the Home Screen with a service worker. */
  supported(): boolean;
  requestPermission(): Promise<NotificationPermission>;
  /** Subscribes with the VAPID key. A subscription for another key ends first. */
  subscribe(applicationServerKey: Uint8Array<ArrayBuffer>): Promise<SubscriptionKeys>;
  /** Ends the subscription of this browser, if there is one. */
  unsubscribe(): Promise<void>;
}

/** Why the switch could not turn push on, as the person reads it. */
export class PushProblem extends Error {}

/** The text when the page cannot subscribe: Safari sends push only to an app of the Home Screen. */
export const NOT_SUPPORTED =
  'This page cannot get notifications. In Safari, add neohar to the Home Screen and open it from there.';

/** The text when the person refused the permission. */
export const DENIED =
  'Notifications are off for neohar. Turn them on in the Settings app of the phone, then try again.';

/** The text while the core has no VAPID key: its listener does not run. */
export const NO_KEY = 'The Mac has no key for notifications yet. Try again in a moment.';

/** The bytes of a base64url text, with or without padding. */
export function keyBytes(text: string): Uint8Array<ArrayBuffer> {
  const base64 = text.replace(/-/g, '+').replace(/_/g, '/').replace(/=+$/, '');
  const padded = base64 + '='.repeat((4 - (base64.length % 4)) % 4);
  const raw = atob(padded);
  const bytes = new Uint8Array(new ArrayBuffer(raw.length));
  for (let at = 0; at < raw.length; at += 1) bytes[at] = raw.charCodeAt(at);
  return bytes;
}

/** Asks for the permission, subscribes and sends the subscription to the core. */
export async function turnOn(
  browser: PushBrowser,
  transport: Transport,
  vapidKey: string | null | undefined,
): Promise<void> {
  if (!browser.supported()) throw new PushProblem(NOT_SUPPORTED);
  if (!vapidKey) throw new PushProblem(NO_KEY);
  const permission = await browser.requestPermission();
  if (permission !== 'granted') throw new PushProblem(DENIED);
  const keys = await browser.subscribe(keyBytes(vapidKey));
  await transport.call('phone.push_subscribe', keys);
}

/** Ends the subscription in the browser and in the core. */
export async function turnOff(browser: PushBrowser, transport: Transport): Promise<void> {
  if (browser.supported()) await browser.unsubscribe();
  await transport.call('phone.push_unsubscribe', {});
}

/** The base64url text of the bytes of a key. */
function base64url(buffer: ArrayBuffer | null): string {
  if (!buffer) return '';
  let raw = '';
  for (const byte of new Uint8Array(buffer)) raw += String.fromCharCode(byte);
  return btoa(raw).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** The browser of the page. */
export function realPushBrowser(): PushBrowser {
  const registration = (): Promise<ServiceWorkerRegistration> => navigator.serviceWorker.ready;
  return {
    supported: () =>
      typeof navigator !== 'undefined' &&
      'serviceWorker' in navigator &&
      typeof window !== 'undefined' &&
      'PushManager' in window &&
      'Notification' in window,
    requestPermission: () => Notification.requestPermission(),
    async subscribe(applicationServerKey) {
      const manager = (await registration()).pushManager;
      const old = await manager.getSubscription();
      if (old) await old.unsubscribe();
      const subscription = await manager.subscribe({ userVisibleOnly: true, applicationServerKey });
      return {
        endpoint: subscription.endpoint,
        p256dh: base64url(subscription.getKey('p256dh')),
        auth: base64url(subscription.getKey('auth')),
      };
    },
    async unsubscribe() {
      const subscription = await (await registration()).pushManager.getSubscription();
      if (subscription) await subscription.unsubscribe();
    },
  };
}
