import { test, expect } from '@playwright/test';
import { registerUser, createChannel, sendMessage, getChannelIdFromUrl } from './helpers';

test.describe('Chat', () => {
  test('send text message and see it appear', async ({ page }) => {
    await registerUser(page, 'chatmsg');
    await createChannel(page, 'ChatTest');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'Hello from e2e!');
    await expect(page.locator('.message-bubble')).toContainText('Hello from e2e!', { timeout: 5000 });
  });

  test('send multiple messages in order', async ({ page }) => {
    await registerUser(page, 'chatmult');
    await createChannel(page, 'MultiChat');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'First');
    await sendMessage(page, 'Second');
    await sendMessage(page, 'Third');
    const bubbles = page.locator('.message-bubble');
    await expect(bubbles).toHaveCount(3, { timeout: 5000 });
    await expect(bubbles.nth(0)).toContainText('First');
    await expect(bubbles.nth(2)).toContainText('Third');
  });

  test('empty message state', async ({ page }) => {
    await registerUser(page, 'chatempty');
    await createChannel(page, 'EmptyChat');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await expect(page.locator('text=No messages yet')).toBeVisible({ timeout: 5000 });
  });

  test('message input clears after send', async ({ page }) => {
    await registerUser(page, 'chatclear');
    await createChannel(page, 'ClearTest');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    const input = page.locator('.message-input textarea, textarea[placeholder*="#"]').first();
    await input.fill('Ephemeral message');
    await input.press('Enter');
    await page.waitForTimeout(300);
    await expect(input).toHaveValue('', { timeout: 3000 });
  });
});
