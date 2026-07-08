// VAST Web Push Service Worker
// Served as-is from frontend/public/ — no bundling, no imports

/**
 * Convert a base64-encoded URL-safe string to a Uint8Array.
 * Needed by pushManager.subscribe() for the VAPID application server key.
 */
function urlBase64ToUint8Array(base64String) {
  const padding = '='.repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding)
    .replace(/-/g, '+')
    .replace(/_/g, '/');

  const rawData = atob(base64);
  const output = new Uint8Array(rawData.length);

  for (let i = 0; i < rawData.length; ++i) {
    output[i] = rawData.charCodeAt(i);
  }

  return output;
}

// ──────────────────────────────────────────────
// Push event
// ──────────────────────────────────────────────
self.addEventListener('push', (event) => {
  event.waitUntil(
    (async () => {
      let payload = {};

      try {
        if (event.data) {
          payload = event.data.json();
        }
      } catch {
        // JSON parse failed — fall through to generic notification
      }

      const channelId = payload.channel_id || '';
      const senderName = payload.sender_name || '';
      const preview = payload.preview || '';
      const url = payload.url || (channelId ? `/channels/${channelId}` : '/');

      const title = senderName || 'New message';
      const options = {
        body: preview || 'New message in VAST',
        icon: '/favicon.ico',
        tag: channelId || 'vast-message',
        data: { url },
      };

      await self.registration.showNotification(title, options);
    })()
  );
});

// ──────────────────────────────────────────────
// Notification click
// ──────────────────────────────────────────────
self.addEventListener('notificationclick', (event) => {
  event.notification.close();

  const url = (event.notification.data && event.notification.data.url) || '/';

  event.waitUntil(
    clients
      .matchAll({ type: 'window', includeUncontrolled: true })
      .then((windowClients) => {
        // Try to focus an existing tab that already shows the target URL
        for (const client of windowClients) {
          if (client.url === url && 'focus' in client) {
            return client.focus();
          }
        }

        // Fall back to the first available window client
        for (const client of windowClients) {
          if ('focus' in client) {
            return client.focus();
          }
        }

        // No window client available — open a new one
        return clients.openWindow(url);
      })
  );
});

// ──────────────────────────────────────────────
// Push subscription change
// ──────────────────────────────────────────────
self.addEventListener('pushsubscriptionchange', (event) => {
  event.waitUntil(
    (async () => {
      const oldSubscription = event.oldSubscription;

      // Unsubscribe the old subscription
      if (oldSubscription) {
        try {
          await oldSubscription.unsubscribe();
        } catch {
          // Best-effort unsubscribe — proceed regardless
        }
      }

      try {
        // Fetch the VAPID public key needed to subscribe
        const keyResponse = await fetch('/api/push/vapid-public-key');
        const { publicKey } = await keyResponse.json();

        const newSubscription = await self.registration.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: urlBase64ToUint8Array(publicKey),
        });

        const subscriptionData = newSubscription.toJSON();

        await fetch('/api/push/resubscribe', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            old_endpoint: oldSubscription ? oldSubscription.endpoint : '',
            new_endpoint: newSubscription.endpoint,
            new_p256dh: subscriptionData.keys.p256dh,
            new_auth: subscriptionData.keys.auth,
          }),
        });
      } catch (err) {
        console.error('pushsubscriptionchange failed:', err);
      }
    })()
  );
});
