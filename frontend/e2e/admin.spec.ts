// Admin Console — end-to-end flow.
//
// Covers: admin login (UI), invite-code CRUD (admin API), audit-log
// visibility (UI), and the token-epoch forced-logout guarantee
// (user token rejected after admin disables the user).
//
// PREREQUISITES (set these before running):
//   - Backend running on :3000 (rtk cargo run) with ADMIN_PASSWORD env set.
//   - Frontend dev server on :5173 (rtk bun run dev), OR start both via
//     `E2E_START_SERVER=1 rtk bun run test:e2e`.
//   - `ADMIN_USERNAME` (default "admin") and `ADMIN_PASSWORD` must be set
//     in the backend environment. The backend returns 403 for admin login
//     when ADMIN_PASSWORD is empty (admin backend disabled).
//   - App.tsx must wire `/admin/audit-logs` to AdminAuditLogsPage (T13
//     wiring task). Until then the audit-logs UI assertions are skipped
//     and only the API-backed checks run.
import { test, expect, type APIRequestContext } from '@playwright/test'

const ADMIN_USERNAME = process.env.ADMIN_USERNAME ?? 'admin'
const ADMIN_PASSWORD = process.env.ADMIN_PASSWORD ?? ''

// Admin tokens are persisted under this localStorage key by adminAuthStore.
const ADMIN_STORAGE_KEY = 'admin-auth-storage'

async function adminLoginViaApi(
  request: APIRequestContext,
): Promise<{ access_token: string }> {
  const res = await request.post('/api/admin/login', {
    data: { username: ADMIN_USERNAME, password: ADMIN_PASSWORD },
    failOnStatusCode: false,
  })
  expect(res.ok(), `admin login failed: ${res.status()}`).toBe(true)
  return res.json()
}

async function getAdminTokenFromUi(page: import('@playwright/test').Page): Promise<string> {
  const raw = await page.evaluate((key) => localStorage.getItem(key), ADMIN_STORAGE_KEY)
  expect(raw, 'admin token missing from localStorage after UI login').not.toBeNull()
  const parsed = JSON.parse(raw!) as {
    state?: { adminToken?: string }
  }
  const token = parsed.state?.adminToken
  expect(token, 'adminToken absent in persisted store').toBeTruthy()
  return token!
}

test.describe('Admin console', () => {
  test.describe.configure({ mode: 'serial' })

  test('admin can log in via the UI', async ({ page }) => {
    test.skip(!ADMIN_PASSWORD, 'ADMIN_PASSWORD env var not set')

    await page.goto('/admin/login')
    await expect(page.locator('h1')).toContainText('Admin Console')

    await page.fill('#admin-username', ADMIN_USERNAME)
    await page.fill('#admin-password', ADMIN_PASSWORD)
    await page.click('button[type="submit"]')

    await page.waitForURL(/\/admin$/, { timeout: 15000 })
    await expect(page).toHaveURL(/\/admin$/)
  })

  test('dashboard renders stat cards after login', async ({ page }) => {
    test.skip(!ADMIN_PASSWORD, 'ADMIN_PASSWORD env var not set')

    await page.goto('/admin/login')
    await page.fill('#admin-username', ADMIN_USERNAME)
    await page.fill('#admin-password', ADMIN_PASSWORD)
    await page.click('button[type="submit"]')
    await page.waitForURL(/\/admin$/, { timeout: 15000 })

    // Dashboard heading + at least one stat card label.
    await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible({ timeout: 10000 })
    await expect(page.getByText('Total Users')).toBeVisible({ timeout: 10000 })
  })

  test('invite-code create/delete is reflected in audit logs', async ({ page, request }) => {
    test.skip(!ADMIN_PASSWORD, 'ADMIN_PASSWORD env var not set')

    // UI login to seed the admin session, then reuse the persisted token
    // for direct admin API calls (the T12 invite-codes page is not wired
    // yet, so we exercise the endpoint directly).
    await page.goto('/admin/login')
    await page.fill('#admin-username', ADMIN_USERNAME)
    await page.fill('#admin-password', ADMIN_PASSWORD)
    await page.click('button[type="submit"]')
    await page.waitForURL(/\/admin$/, { timeout: 15000 })

    const token = await getAdminTokenFromUi(page)
    const authHeaders = { Authorization: `Bearer ${token}` }

    const code = `e2e-${Date.now()}`
    const createRes = await request.post('/api/admin/invite-codes', {
      headers: authHeaders,
      data: { code, max_uses: 1, is_active: true },
    })
    expect(createRes.ok(), `create invite failed: ${createRes.status()}`).toBe(true)

    // Audit log should now mention the invite.create action for this code.
    const afterCreate = await request.get('/api/admin/audit-logs', {
      headers: authHeaders,
      params: { action: 'invite.create', page: '1', limit: '50' },
    })
    expect(afterCreate.ok()).toBe(true)
    const afterCreateBody = await afterCreate.json()
    const createMatch = afterCreateBody.find(
      (row: { target_id?: string }) => row.target_id === code,
    )
    expect(createMatch, `audit log missing invite.create row for ${code}`).toBeTruthy()

    // Delete the code, then verify invite.delete is audited.
    const delRes = await request.delete(`/api/admin/invite-codes/${code}`, {
      headers: authHeaders,
    })
    expect(delRes.ok(), `delete invite failed: ${delRes.status()}`).toBe(true)

    const afterDelete = await request.get('/api/admin/audit-logs', {
      headers: authHeaders,
      params: { action: 'invite.delete', page: '1', limit: '50' },
    })
    expect(afterDelete.ok()).toBe(true)
    const afterDeleteBody = await afterDelete.json()
    const deleteMatch = afterDeleteBody.find(
      (row: { target_id?: string }) => row.target_id === code,
    )
    expect(deleteMatch, `audit log missing invite.delete row for ${code}`).toBeTruthy()
  })

  test('audit-logs page surfaces the invite.create action in the UI', async ({ page, request }) => {
    test.skip(!ADMIN_PASSWORD, 'ADMIN_PASSWORD env var not set')

    // Requires App.tsx to wire /admin/audit-logs to AdminAuditLogsPage.
    // The orchestrator runs this in the F3 verification wave after the
    // T13 wiring follow-up lands; until then it fails with a missing
    // table, which is the intended signal.

    await page.goto('/admin/login')
    await page.fill('#admin-username', ADMIN_USERNAME)
    await page.fill('#admin-password', ADMIN_PASSWORD)
    await page.click('button[type="submit"]')
    await page.waitForURL(/\/admin$/, { timeout: 15000 })

    const token = await getAdminTokenFromUi(page)
    const code = `e2e-ui-${Date.now()}`
    await request.post('/api/admin/invite-codes', {
      headers: { Authorization: `Bearer ${token}` },
      data: { code, max_uses: 1, is_active: true },
    })

    await page.goto('/admin/audit-logs')
    await expect(page.getByRole('heading', { name: 'Audit Logs' })).toBeVisible({ timeout: 10000 })
    await expect(
      page.locator('table').filter({ hasText: 'invite.create' }),
    ).toBeVisible({ timeout: 10000 })

    await request.delete(`/api/admin/invite-codes/${code}`, {
      headers: { Authorization: `Bearer ${token}` },
    })
  })
})

test.describe('Admin forced-logout (token epoch)', () => {
  test('disabled user token is rejected on /api/auth/profile', async ({ request }) => {
    test.skip(!ADMIN_PASSWORD, 'ADMIN_PASSWORD env var not set')

    // 1. Register a fresh user (idempotent — 409 is fine if it exists).
    const username = `e2eforce${Date.now()}`
    const password = 'E2eForce1!'
    await request.post('/api/auth/register', {
      data: { username, password, invite_code: 'IM2024' },
      failOnStatusCode: false,
    })

    // 2. Log in to mint a valid user access token.
    const loginRes = await request.post('/api/auth/login', {
      data: { username, password },
    })
    expect(loginRes.ok(), `user login failed: ${loginRes.status()}`).toBe(true)
    const { access_token: userToken } = await loginRes.json()

    // Sanity: the fresh token works against a protected user endpoint.
    const profileOk = await request.get('/api/auth/profile', {
      headers: { Authorization: `Bearer ${userToken}` },
    })
    expect(profileOk.ok()).toBe(true)

    // 3. Admin disables the user (bumps token_epoch). We need the user id,
    //    which /api/auth/profile returns.
    const profileBody = await profileOk.json()
    const userId = profileBody.id
    expect(userId, 'profile response missing id').toBeTruthy()

    const admin = await adminLoginViaApi(request)
    const disableRes = await request.patch(`/api/admin/users/${userId}`, {
      headers: { Authorization: `Bearer ${admin.access_token}` },
      data: { disabled: true },
    })
    expect(disableRes.ok(), `disable failed: ${disableRes.status()}`).toBe(true)

    // 4. The pre-disable token must now be rejected.
    const profileAfter = await request.get('/api/auth/profile', {
      headers: { Authorization: `Bearer ${userToken}` },
      failOnStatusCode: false,
    })
    expect(profileAfter.status()).toBe(401)
  })
})
