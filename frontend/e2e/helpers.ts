import { Page, expect } from '@playwright/test';

const INVITE = 'IM2024';
let counter = 0;

export async function registerUser(page: Page, prefix = 'e2e'): Promise<{username:string;password:string}> {
  counter++;
  const username = `${prefix}${Date.now()}${counter}`;
  const password = 'E2eTest1!';
  await page.goto('/register');
  await page.waitForSelector('#reg-username');
  await page.fill('#reg-username', username);
  await page.fill('#reg-password', password);
  await page.fill('#reg-invite-code', INVITE);
  await page.click('button[type="submit"]');
  await page.waitForURL(/\/channels/, { timeout: 15000 });
  return { username, password };
}

export async function loginUser(page: Page, username: string, password: string) {
  await page.goto('/login');
  await page.waitForSelector('#login-username');
  await page.fill('#login-username', username);
  await page.fill('#login-password', password);
  await page.click('button[type="submit"]');
  await page.waitForURL(/\/channels/, { timeout: 15000 });
}

export async function logoutUser(page: Page) {
  await page.evaluate(() => localStorage.removeItem('auth-storage'));
  await page.goto('/login');
}

export async function createChannel(page: Page, name: string): Promise<string> {
  let promptCount = 0;
  page.on('dialog', async (dialog) => {
    promptCount++;
    if (promptCount === 1) await dialog.accept(name);
    else await dialog.accept('');
  });
  await page.click('[aria-label="Create channel"]');
  await page.waitForTimeout(500);
  page.removeAllListeners('dialog');
  return name;
}

export async function sendMessage(page: Page, text: string) {
  const input = page.locator('.message-input textarea, textarea[placeholder*="#"]').first();
  await input.fill(text);
  await input.press('Enter');
  await page.waitForTimeout(300);
}

export function getChannelIdFromUrl(page: Page): string {
  const url = page.url();
  const parts = url.split('/channels/');
  return parts[1]?.split('/')[0] ?? '';
}
