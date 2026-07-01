import { test, expect } from '@playwright/test';
import { registerUser, createChannel, getChannelIdFromUrl } from './helpers';

test.describe('Channels', () => {
  test('create channel and see it in sidebar', async ({ page }) => {
    await registerUser(page, 'chcr');
    const name = await createChannel(page, 'MyTestChannel');
    await page.waitForSelector('.channel-item');
    await expect(page.locator('.channel-item').filter({ hasText: name })).toBeVisible({ timeout: 5000 });
  });

  test('navigate to created channel', async ({ page }) => {
    await registerUser(page, 'chnav');
    const name = await createChannel(page, 'NavChannel');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').filter({ hasText: name }).click();
    await page.waitForTimeout(500);
    expect(page.url()).toContain('/channels/');
  });

  test('empty channels state', async ({ page }) => {
    await registerUser(page, 'chempty');
    await expect(page.locator('text=No channels yet')).toBeVisible({ timeout: 5000 });
  });

  test('select channel prompt when no channel selected', async ({ page }) => {
    await registerUser(page, 'chprompt');
    await expect(page.locator('text=Select a channel')).toBeVisible({ timeout: 5000 });
  });

  test('create multiple channels', async ({ page }) => {
    await registerUser(page, 'chmulti');
    await createChannel(page, 'Channel A');
    await page.waitForTimeout(300);
    await createChannel(page, 'Channel B');
    await page.waitForTimeout(300);
    const items = page.locator('.channel-item');
    await expect(items).toHaveCount(2, { timeout: 5000 });
  });

  test('channel name appears in header', async ({ page }) => {
    await registerUser(page, 'chheader');
    const name = await createChannel(page, 'HeaderTest');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').filter({ hasText: name }).click();
    await page.waitForTimeout(500);
    await expect(page.locator('h1')).toContainText(name);
  });
});
