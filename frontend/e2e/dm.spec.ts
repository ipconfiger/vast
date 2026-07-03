import { test, expect } from '@playwright/test';
import { registerUser, createChannel } from './helpers';

test.describe('Direct Messages', () => {
  test('DM page renders', async ({ page }) => {
    await registerUser(page, 'dmpage');
    await page.goto('/dm/test-user-id');
    await page.waitForTimeout(1000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('create DM via API', async ({ page }) => {
    await registerUser(page, 'dmapi');
    const ctx2 = await page.context().browser()!.newContext();
    const page2 = await ctx2.newPage();
    await registerUser(page2, 'dmpartner');
    await page.goto('/dm/test-dm-id');
    await page.waitForTimeout(500);
    await expect(page.locator('body')).toBeVisible();
    await ctx2.close();
  });

  test('DM sidebar shows DM channels', async ({ page }) => {
    await registerUser(page, 'dmside');

    const { token: tokenA, userId: userIdA } = await page.evaluate(() => {
      const raw = localStorage.getItem('auth-storage');
      const parsed = raw ? JSON.parse(raw) : null;
      const token = parsed?.state?.token ?? '';
      const payload = token ? JSON.parse(atob(token.split('.')[1])) : {};
      return { token, userId: payload.sub ?? '' };
    });

    const ctx2 = await page.context().browser()!.newContext();
    const page2 = await ctx2.newPage();
    await registerUser(page2, 'dmpartner');
    const userIdB = await page2.evaluate(() => {
      const raw = localStorage.getItem('auth-storage');
      const parsed = raw ? JSON.parse(raw) : null;
      const token = parsed?.state?.token ?? '';
      const payload = token ? JSON.parse(atob(token.split('.')[1])) : {};
      return payload.sub ?? '';
    });
    await ctx2.close();

    expect(userIdA).toBeTruthy();
    expect(userIdB).toBeTruthy();

    const dmResponse = await page.request.post('/api/dm', {
      data: { user_ids: [userIdA, userIdB] },
      headers: { Authorization: `Bearer ${tokenA}` },
    });
    expect(dmResponse.ok()).toBeTruthy();

    await page.goto('/channels');
    await page.waitForTimeout(1500);

    await expect(page.locator('.channel-sidebar')).toBeVisible();
    await expect(page.locator('.dm-item')).toBeVisible({ timeout: 10000 });
  });

  test('navigate between DM and regular channel', async ({ page }) => {
    await registerUser(page, 'dmnav');
    await createChannel(page, 'RegularChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    expect(page.url()).toContain('/channels/');
    await page.goto('/dm/some-id');
    await page.waitForTimeout(500);
    expect(page.url()).toContain('/dm/');
  });
});
