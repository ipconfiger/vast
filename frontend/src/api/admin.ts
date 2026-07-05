// Admin Console — API client.
// Standalone module: does NOT import from or modify api/client.ts.
// Reads tokens from adminAuthStore; adminApiClient mirrors the user
// apiClient's 401-refresh-and-retry dedup pattern but against the
// admin endpoints under /api/admin.
import { useAdminAuthStore } from '../stores/adminAuthStore'

const API_BASE = import.meta.env.VITE_API_BASE || '/api'
const ADMIN_BASE = `${API_BASE}/admin`

export class AdminApiClientError extends Error {
  code: string
  status: number

  constructor(code: string, message: string, status: number) {
    super(message)
    this.code = code
    this.status = status
    this.name = 'AdminApiClientError'
  }
}

// --- Types (frozen route contract — see src/api/admin/mod.rs doc-comment) ---

export interface DashboardStats {
  total_users: number
  active_sessions_24h: number
  total_channels: number
  total_messages: number
  total_invite_codes: number
  active_invite_codes: number
}

export interface AdminUser {
  id: string
  username: string
  display_name: string
  avatar_url: string
  created_at: number
}

export interface InviteCode {
  code: string
  created_by_user_id: string | null
  max_uses: number
  use_count: number
  is_active: boolean
  created_at: number
}

export interface AuditLog {
  id: string
  action: string
  target_type: string | null
  target_id: string | null
  details: string | null
  performed_at: number
}

export interface Bot {
  id: string
  user_id: string
  name: string
  display_name: string
  api_url: string
  system_prompt: string
  model: string
  is_active: boolean
  created_at: number
}

interface AdminTokenPair {
  access_token: string
  refresh_token: string
  expires_in: number
}

// --- Refresh dedup (mirrors client.ts refreshPromise singleton) ---

let adminRefreshPromise: Promise<string | null> | null = null

async function refreshAdminAccessToken(): Promise<string | null> {
  const { adminRefreshToken } = useAdminAuthStore.getState()
  if (!adminRefreshToken) return null

  // Deduplicate concurrent refresh calls
  if (adminRefreshPromise) return adminRefreshPromise

  adminRefreshPromise = (async () => {
    try {
      const data = await adminRefresh(adminRefreshToken)
      useAdminAuthStore.getState().setTokens(data)
      return data.access_token
    } catch {
      useAdminAuthStore.getState().logout()
      return null
    } finally {
      adminRefreshPromise = null
    }
  })()

  return adminRefreshPromise
}

// --- Core fetch wrapper ---

export async function adminApiClient<T>(
  endpoint: string,
  options: RequestInit = {},
): Promise<T> {
  const store = useAdminAuthStore.getState()

  // If the token is expired, try refreshing before the request
  let token = store.adminToken
  if (token && store.isTokenExpired()) {
    const newToken = await refreshAdminAccessToken()
    if (!newToken) {
      throw new AdminApiClientError(
        'UNAUTHORIZED',
        'Admin session expired. Please log in again.',
        401,
      )
    }
    token = newToken
  }

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string> | undefined),
  }
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  const response = await fetch(`${ADMIN_BASE}${endpoint}`, {
    ...options,
    headers,
  })

  // On 401, try refreshing the token once and retry
  if (response.status === 401) {
    const newToken = await refreshAdminAccessToken()
    if (newToken) {
      headers['Authorization'] = `Bearer ${newToken}`
      const retryResponse = await fetch(`${ADMIN_BASE}${endpoint}`, {
        ...options,
        headers,
      })
      if (!retryResponse.ok) {
        throw await buildError(retryResponse)
      }
      // 204 responses have no body
      if (retryResponse.status === 204) return undefined as T
      return retryResponse.json()
    }
    store.logout()
    throw new AdminApiClientError(
      'UNAUTHORIZED',
      'Admin session expired. Please log in again.',
      401,
    )
  }

  if (!response.ok) {
    throw await buildError(response)
  }

  if (response.status === 204) return undefined as T
  return response.json()
}

async function buildError(response: Response): Promise<AdminApiClientError> {
  const body = await response.json().catch(() => ({}))
  const code = body.code ?? body.error?.code ?? 'ERROR'
  const message = body.message ?? body.error?.message ?? 'Request failed'
  return new AdminApiClientError(code, message, response.status)
}

// --- Auth endpoints (standalone fetch — no adminApiClient, no auth header) ---

export async function adminLogin(
  username: string,
  password: string,
): Promise<AdminTokenPair> {
  const response = await fetch(`${ADMIN_BASE}/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  })
  if (!response.ok) {
    throw await buildError(response)
  }
  return response.json()
}

export async function adminRefresh(
  refreshToken: string,
): Promise<AdminTokenPair> {
  const response = await fetch(`${ADMIN_BASE}/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken }),
  })
  if (!response.ok) {
    throw await buildError(response)
  }
  return response.json()
}

export async function adminLogout(): Promise<void> {
  const { adminToken, logout } = useAdminAuthStore.getState()
  if (!adminToken) {
    logout()
    return
  }
  try {
    await fetch(`${ADMIN_BASE}/logout`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${adminToken}`,
      },
    })
  } finally {
    // Stateless backend: always clear local state regardless of response.
    logout()
  }
}

export async function adminMe(): Promise<{ username: string }> {
  return adminApiClient<{ username: string }>('/me')
}

// --- Dashboard ---

export async function getDashboard(): Promise<DashboardStats> {
  return adminApiClient<DashboardStats>('/dashboard')
}

// --- Users ---

export async function listUsers(
  params?: { page?: number; limit?: number; q?: string },
): Promise<AdminUser[]> {
  const qs = buildQuery(params)
  return adminApiClient<AdminUser[]>(`/users${qs}`)
}

export async function getUser(id: string): Promise<AdminUser> {
  return adminApiClient<AdminUser>(`/users/${encodeURIComponent(id)}`)
}

export async function updateUser(
  id: string,
  body: { display_name?: string; disabled?: boolean },
): Promise<AdminUser> {
  return adminApiClient<AdminUser>(
    `/users/${encodeURIComponent(id)}`,
    { method: 'PATCH', body: JSON.stringify(body) },
  )
}

export async function resetUserPassword(
  id: string,
  body: { new_password: string },
): Promise<void> {
  await adminApiClient<void>(`/users/${encodeURIComponent(id)}/reset-password`, {
    method: 'POST',
    body: JSON.stringify(body),
  })
}

export async function deleteUser(id: string): Promise<void> {
  await adminApiClient<void>(`/users/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  })
}

// --- Invite codes ---

export async function listInviteCodes(
  params?: { page?: number; limit?: number },
): Promise<InviteCode[]> {
  const qs = buildQuery(params)
  return adminApiClient<InviteCode[]>(`/invite-codes${qs}`)
}

export async function createInviteCode(body: {
  code: string
  max_uses?: number
  is_active?: boolean
}): Promise<InviteCode> {
  return adminApiClient<InviteCode>('/invite-codes', {
    method: 'POST',
    body: JSON.stringify(body),
  })
}

export async function updateInviteCode(
  code: string,
  body: {
    max_uses?: number
    is_active?: boolean
    reset_use_count?: boolean
  },
): Promise<InviteCode> {
  return adminApiClient<InviteCode>(
    `/invite-codes/${encodeURIComponent(code)}`,
    { method: 'PATCH', body: JSON.stringify(body) },
  )
}

export async function deleteInviteCode(code: string): Promise<void> {
  await adminApiClient<void>(`/invite-codes/${encodeURIComponent(code)}`, {
    method: 'DELETE',
  })
}

// --- Bots ---

export async function listBots(): Promise<Bot[]> {
  return adminApiClient<Bot[]>('/bots')
}

export async function createBot(body: {
  name: string
  display_name?: string
  api_url: string
  api_key?: string
  system_prompt?: string
  model?: string
}): Promise<Bot> {
  return adminApiClient<Bot>('/bots', {
    method: 'POST',
    body: JSON.stringify(body),
  })
}

export async function updateBot(
  id: string,
  body: Partial<{
    display_name: string
    api_url: string
    api_key: string
    system_prompt: string
    model: string
    is_active: boolean
  }>,
): Promise<Bot> {
  return adminApiClient<Bot>(`/bots/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: JSON.stringify(body),
  })
}

export async function deleteBot(id: string): Promise<void> {
  await adminApiClient<void>(`/bots/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  })
}

export interface BotTestResult {
  ok: boolean
  response?: string
  error?: string
}

export async function testBot(id: string): Promise<BotTestResult> {
  return adminApiClient<BotTestResult>(
    `/bots/${encodeURIComponent(id)}/test`,
    { method: 'POST' },
  )
}

// --- Audit logs ---

export async function listAuditLogs(
  params?: { page?: number; limit?: number; action?: string },
): Promise<AuditLog[]> {
  const qs = buildQuery(params)
  return adminApiClient<AuditLog[]>(`/audit-logs${qs}`)
}

// --- Helpers ---

function buildQuery(
  params?: Record<string, number | string | undefined>,
): string {
  if (!params) return ''
  const entries = Object.entries(params).filter(
    ([, v]) => v !== undefined && v !== null,
  )
  if (entries.length === 0) return ''
  const sp = new URLSearchParams()
  for (const [k, v] of entries) sp.set(k, String(v))
  return `?${sp.toString()}`
}
