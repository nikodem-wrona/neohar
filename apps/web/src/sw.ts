// The service worker of the phone app. It takes the page at once and keeps
// nothing: the app needs the Mac, so it has no copy for a time without network.
//
// A push shows its notification and sets the badge of the app. The core makes the whole text, and
// the worker shows it as it comes. iOS ends a subscription whose pushes show nothing, so each push
// shows a notification. A tap on the notification opens the app at the view of the push, for
// example `#/run/<id>`.

/** The JSON of a push, as the core sends it. */
interface Notice {
  title?: string;
  body?: string;
  tag?: string;
  url?: string;
  badge?: number;
}

interface Waiting {
  waitUntil(promise: Promise<unknown>): void;
}

interface PushEventLike extends Waiting {
  data: { json(): unknown } | null;
}

interface NotificationEventLike extends Waiting {
  notification: { data: unknown; close(): void };
}

interface WindowClientLike {
  url: string;
  focus(): Promise<unknown>;
  navigate(url: string): Promise<unknown>;
}

/** What this worker uses of `ServiceWorkerGlobalScope`. */
interface Worker {
  addEventListener(type: 'install' | 'activate', listener: (event: Waiting) => void): void;
  addEventListener(type: 'push', listener: (event: PushEventLike) => void): void;
  addEventListener(
    type: 'notificationclick',
    listener: (event: NotificationEventLike) => void,
  ): void;
  skipWaiting(): Promise<void>;
  clients: {
    claim(): Promise<void>;
    matchAll(options: {
      type: 'window';
      includeUncontrolled: boolean;
    }): Promise<WindowClientLike[]>;
    openWindow(url: string): Promise<unknown>;
  };
  registration: {
    scope: string;
    showNotification(
      title: string,
      options: { body: string; tag: string; data: unknown },
    ): Promise<void>;
  };
  navigator: { setAppBadge?(count: number): Promise<void> };
}

const worker = self as unknown as Worker;

worker.addEventListener('install', (event) => event.waitUntil(worker.skipWaiting()));
worker.addEventListener('activate', (event) => event.waitUntil(worker.clients.claim()));

/** The notice of a push. A push that is not JSON still shows a notification. */
function noticeOf(event: PushEventLike): Notice {
  try {
    const value = event.data?.json();
    return value && typeof value === 'object' ? (value as Notice) : {};
  } catch {
    return {};
  }
}

worker.addEventListener('push', (event) => {
  const notice = noticeOf(event);
  const url = new URL(notice.url ?? '#/', worker.registration.scope).href;
  const shown = worker.registration.showNotification(notice.title ?? 'neohar', {
    body: notice.body ?? '',
    tag: notice.tag ?? 'neohar',
    data: { url },
  });
  // The notification shows also when the system refuses the badge.
  const badge =
    typeof notice.badge === 'number' && worker.navigator.setAppBadge
      ? worker.navigator.setAppBadge(notice.badge).catch(() => undefined)
      : Promise.resolve();
  event.waitUntil(Promise.all([shown, badge]));
});

worker.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const data = event.notification.data as { url?: string } | null;
  const url = data?.url ?? worker.registration.scope;
  event.waitUntil(
    worker.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clients) => {
      const open = clients[0];
      if (!open) return worker.clients.openWindow(url);
      // A failed move still brings the app to the front.
      return open
        .navigate(url)
        .catch(() => undefined)
        .then(() => open.focus());
    }),
  );
});
