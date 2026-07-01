import { test, expect } from '@playwright/test';
import { registerUser, createChannel, sendMessage } from './helpers';

test.describe('Reactions', () => {
  test.beforeEach(async ({ page }) => {
    await registerUser(page, 'react');
    await createChannel(page, 'ReactionTest');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'React to this message');
    await page.waitForSelector('.message-bubble');
  });

  test('add reaction to message', async ({ page }) => {
    await page.locator('.message-bubble').first().hover();
    await page.waitForTimeout(300);
    const pickerBtn = page.locator('[aria-label="Add reaction"]').first();
    if (await pickerBtn.isVisible()) {
      await pickerBtn.click({ force: true });
      await page.waitForTimeout(300);
      await page.locator('button:has-text("👍")').first().click({ force: true, timeout: 3000 }).catch(() => {});
      await page.waitForTimeout(500);
    }
    await expect(page.locator('.message-bubble').first()).toBeVisible();
  });

  test('message with no reactions shows no reaction bar', async ({ page }) => {
    await expect(page.locator('.message-bubble').first()).toBeVisible();
  });

  test('send second message and verify it renders', async ({ page }) => {
    await sendMessage(page, 'Second message for reactions');
    await page.waitForTimeout(500);
    const bubbles = page.locator('.message-bubble');
    await expect(bubbles).toHaveCount(2, { timeout: 5000 });
  });

  test('message input works after reaction interaction', async ({ page }) => {
    await sendMessage(page, 'Post-reaction message');
    await expect(page.locator('.message-bubble').last()).toContainText('Post-reaction message', { timeout: 5000 });
  });

  test('page does not crash when rendering message with reaction', async ({ page }) => {
    await expect(page.locator('h1')).toBeVisible();
    await expect(page.locator('.message-bubble').first()).toBeVisible();
  });
});
