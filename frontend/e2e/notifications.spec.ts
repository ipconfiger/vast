import { test, expect } from '@playwright/test';
import { registerUser } from './helpers';

test.describe('Notifications', () => {
  test('grant notification permission -> subscription stored, toggle shows Disable', async ({ page, context }) => {
    await registerUser(page, 'notif');

    // Mock push API endpoints — set up routes before navigating to profile
    await page.route('**/api/push/vapid-public-key', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          public_key: 'BEl62iUYgUivxIkv69yViEuiBIa-Ib9-SkvMeAtA3LFgDzkrxZJjSgSnfckjBJuBkr3qBUvIHtLflH55EPu_Ml4',
        }),
      }),
    );
    await page.route('**/api/push/subscribe', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );
    await page.route('**/api/push/unsubscribe', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );

    // Grant notification permission so requestPermission() resolves to 'granted'
    await context.grantPermissions(['notifications']);

    // Navigate to profile
    await page.goto('/profile');

    // Mock PushManager.subscribe AFTER page load (must be on this page, not the previous one)
    await page.evaluate(() => {
      const PM = (window as any).PushManager;
      if (PM) {
        PM.prototype.subscribe = () =>
          Promise.resolve({
            endpoint: 'https://test.example.com/fake-endpoint',
            expirationTime: null,
            toJSON: () => ({
              endpoint: 'https://test.example.com/fake-endpoint',
              expirationTime: null,
              keys: {
                p256dh: 'BPnYJxCyB4Sffq42LHTX8EqfCnzmGITBWfYBjLNxwPwOUbw6F7J7jA8aM_tq3EcY3kUMUzYZKl5Qy8RMXFzSpCo',
                auth: 'DzBZ0YDBoHcNyWWZfH4PTw',
              },
            }),
            unsubscribe: () => Promise.resolve(true),
            getKey: () => new ArrayBuffer(0),
          });
      }
    });

    // Click Enable Browser Notifications
    const enableBtn = page.getByRole('button', { name: /Enable Browser Notifications/i });
    await enableBtn.waitFor({ state: 'visible', timeout: 10000 });
    await enableBtn.click();

    // After successful subscription, button changes to "Disable Notifications" (red)
    const disableBtn = page.getByRole('button', { name: /Disable Notifications/i });
    await disableBtn.waitFor({ state: 'visible', timeout: 10000 });
    await expect(disableBtn).toBeVisible();
  });

  test('deny notification permission -> blocked message visible', async ({ page }) => {
    await registerUser(page, 'notifdeny');

    // Navigate to profile
    await page.goto('/profile');

    // Click enable — without grantPermissions, headless Chromium auto-denies
    const enableBtn = page.getByRole('button', { name: /Enable Browser Notifications/i });
    await enableBtn.waitFor({ state: 'visible', timeout: 10000 });
    await enableBtn.click();

    // Verify the denied/permission-blocked message is shown
    const deniedMsg = page.getByText(/Notification permission was denied/i);
    await deniedMsg.waitFor({ state: 'visible', timeout: 10000 });
    await expect(deniedMsg).toBeVisible();
  });
});
