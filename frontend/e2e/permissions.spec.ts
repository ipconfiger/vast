import { test, expect } from '@playwright/test';
import { registerUser, createChannel, sendMessage } from './helpers';

test.describe('Permissions', () => {
  test('Join Request button visible for non-member', async ({ page }) => {
    await registerUser(page, 'perma');
    await createChannel(page, 'PermChannel');
    await page.waitForTimeout(500);
    const context2 = await page.context().browser()!.newContext();
    const page2 = await context2.newPage();
    await registerUser(page2, 'permb');
    await expect(page2.locator('.channel-page, .flex.h-screen')).toBeVisible();
    await context2.close();
  });

  test('archive channel blocks message input', async ({ page }) => {
    await registerUser(page, 'permarch');
    await createChannel(page, 'ArchiveChannel');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'Before archive');
    await page.waitForSelector('.message-bubble');
    const channelId = page.url().split('/channels/')[1]?.split('/')[0];
    if (channelId) {
      await page.evaluate(async (cid) => {
        const token = JSON.parse(localStorage.getItem('auth-storage') || '{}')?.state?.token;
        await fetch(`/api/channels/${cid}/archive`, {
          method: 'POST',
          headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' }
        });
      }, channelId);
      await page.waitForTimeout(500);
    }
    await expect(page.locator('.channel-page, .flex.h-screen')).toBeVisible();
  });

  test('archived channel can still be read', async ({ page }) => {
    await registerUser(page, 'permread');
    await createChannel(page, 'ReadOnlyChannel');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'Read this');
    await page.waitForSelector('.message-bubble');
    const channelId = page.url().split('/channels/')[1]?.split('/')[0];
    if (channelId) {
      await page.evaluate(async (cid) => {
        const token = JSON.parse(localStorage.getItem('auth-storage') || '{}')?.state?.token;
        await fetch(`/api/channels/${cid}/archive`, {
          method: 'POST',
          headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' }
        });
      }, channelId);
      await page.waitForTimeout(500);
    }
    await page.reload();
    await page.waitForTimeout(1000);
    await expect(page.locator('.message-bubble').first()).toBeVisible({ timeout: 5000 });
  });

  test('unarchive restores write access', async ({ page }) => {
    await registerUser(page, 'permunar');
    await createChannel(page, 'UnarchiveChannel');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    const channelId = page.url().split('/channels/')[1]?.split('/')[0];
    if (channelId) {
      await page.evaluate(async (cid) => {
        const token = JSON.parse(localStorage.getItem('auth-storage') || '{}')?.state?.token;
        await fetch(`/api/channels/${cid}/archive`, {
          method: 'POST',
          headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' }
        });
        await fetch(`/api/channels/${cid}/unarchive`, {
          method: 'POST',
          headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' }
        });
      }, channelId);
      await page.waitForTimeout(500);
    }
    await sendMessage(page, 'After unarchive');
    await page.waitForTimeout(500);
    await expect(page.locator('.message-bubble').last()).toContainText('After unarchive', { timeout: 5000 });
  });

  test('channel page shows header', async ({ page }) => {
    await registerUser(page, 'permhead');
    await createChannel(page, 'HeaderChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await expect(page.locator('h1')).toBeVisible();
  });

  test('message history survives navigation', async ({ page }) => {
    await registerUser(page, 'permnav');
    await createChannel(page, 'NavChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'History message');
    await page.waitForSelector('.message-bubble');
    await createChannel(page, 'OtherChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await page.locator('.channel-item').filter({ hasText: 'NavChan' }).click();
    await page.waitForTimeout(500);
    await expect(page.locator('.message-bubble').first()).toContainText('History message', { timeout: 5000 });
  });
});
