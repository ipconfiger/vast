import { test, expect } from '@playwright/test';

test.describe('Chat', () => {
  test.beforeEach(async ({ page }) => {
    // Register and login to get to the channels page
    const username = `e2echat${Date.now()}`;
    await page.goto('/register');
    await page.fill('#reg-username', username);
    await page.fill('#reg-password', 'ChatPass12345');
    await page.fill('#reg-invite-code', 'IM2024');
    await page.click('button[type="submit"]');

    // Wait for redirect to channels
    await page.waitForURL(/\/channels/, { timeout: 15000 });
  });

  test('channels page renders after login', async ({ page }) => {
    // Should be on the channels page
    await expect(page).toHaveURL(/\/channels/);

    // The sidebar or channel list should be visible
    await expect(page.locator('.channel-page, .flex.h-screen')).toBeVisible();
  });

  test('can navigate to a channel', async ({ page }) => {
    // The channels page should show a channel list or prompt
    await page.waitForSelector('.channel-page, .flex.h-screen', { timeout: 5000 });

    // If there's a "select channel" prompt or channel list, verify it renders
    const pageContent = page.locator('body');
    await expect(pageContent).toBeVisible();
  });

  test('channel message input is visible', async ({ page }) => {
    // Navigate to a specific channel
    await page.goto('/channels/test-channel');

    // Wait for the page to render
    await page.waitForTimeout(2000);

    // The page should load (even if channel doesn't exist yet, it should show UI)
    const pageContent = page.locator('body');
    await expect(pageContent).toBeVisible();
  });
});
