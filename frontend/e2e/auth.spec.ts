import { test, expect } from '@playwright/test';

test.describe('Authentication', () => {
  test('login page renders and redirects to /channels on success', async ({ page }) => {
    // Navigate to login page
    await page.goto('/login');
    await expect(page.locator('h1')).toContainText('Welcome back');

    // Fill in credentials
    await page.fill('#login-username', 'e2elogintest');
    await page.fill('#login-password', 'E2eLogin123');

    // Submit the form
    await page.click('button[type="submit"]');

    // Should redirect to /channels on success
    await page.waitForURL(/\/channels/, { timeout: 15000 });
    await expect(page).toHaveURL(/\/channels/);
  });

  test('login page shows error for bad credentials', async ({ page }) => {
    await page.goto('/login');

    await page.fill('#login-username', 'nonexistentuser123');
    await page.fill('#login-password', 'WrongPass1234');

    await page.click('button[type="submit"]');

    // Should show an error message
    await expect(page.locator('.bg-red-500\\/10, [class*="red"]')).toBeVisible({ timeout: 10000 });
  });

  test('login page has link to register', async ({ page }) => {
    await page.goto('/login');

    const registerLink = page.locator('a[href="/register"]');
    await expect(registerLink).toBeVisible();

    // Click the link
    await registerLink.click();
    await expect(page).toHaveURL('/register');
    await expect(page.locator('h1')).toContainText('Create account');
  });

  test('register page renders form fields', async ({ page }) => {
    await page.goto('/register');

    // All three fields should be visible
    await expect(page.locator('#reg-username')).toBeVisible();
    await expect(page.locator('#reg-password')).toBeVisible();
    await expect(page.locator('#reg-invite-code')).toBeVisible();

    // Submit button should be present
    await expect(page.locator('button[type="submit"]')).toBeVisible();

    // Should have link to login
    await expect(page.locator('a[href="/login"]')).toBeVisible();
  });

  test('register page validates required fields', async ({ page }) => {
    await page.goto('/register');

    // Submit empty form
    await page.click('button[type="submit"]');

    // Should show validation errors for empty fields
    await expect(page.locator('.text-red-400').first()).toBeVisible({ timeout: 5000 });
  });

  test('register page redirects to /channels on success', async ({ page }) => {
    await page.goto('/register');

    // Use a unique username to avoid conflicts
    const username = `e2ereg${Date.now()}`;

    await page.fill('#reg-username', username);
    await page.fill('#reg-password', 'E2eRegPass123');
    await page.fill('#reg-invite-code', 'IM2024');

    await page.click('button[type="submit"]');

    // Should redirect to /channels on success
    await page.waitForURL(/\/channels/, { timeout: 15000 });
    await expect(page).toHaveURL(/\/channels/);
  });

  test('unauthenticated user is redirected to /login', async ({ page }) => {
    await page.goto('/channels');

    // Should be redirected to login
    await page.waitForURL('/login', { timeout: 10000 });
    await expect(page.locator('h1')).toContainText('Welcome back');
  });
});
