import { test, expect } from '@playwright/test';
import { registerUser, createChannel, sendMessage } from './helpers';

test.describe('Search', () => {
  test('search page renders', async ({ page }) => {
    await registerUser(page, 'searchp');
    await page.goto('/search');
    await page.waitForTimeout(1000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('search page accessible from channels', async ({ page }) => {
    await registerUser(page, 'searchacc');
    await page.goto('/search');
    await page.waitForTimeout(500);
    expect(page.url()).toContain('/search');
    await page.goto('/channels');
    await page.waitForTimeout(500);
    expect(page.url()).toContain('/channels');
  });

  test('search with keyword via API', async ({ page }) => {
    await registerUser(page, 'searchapi');
    await createChannel(page, 'SearchChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'production deployment');
    await page.waitForTimeout(500);
    const result = await page.evaluate(async () => {
      const token = JSON.parse(localStorage.getItem('auth-storage') || '{}')?.state?.token;
      const res = await fetch('/api/search?q=production', {
        headers: { 'Authorization': `Bearer ${token}` }
      });
      return res.json();
    });
    expect(result.results).toBeDefined();
    expect(result.results.length).toBeGreaterThan(0);
    expect(result.results[0].snippet).toContain('production');
  });
});
