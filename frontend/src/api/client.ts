import { useAuthStore } from '../stores/authStore'

// Use VITE_API_BASE env var when set, otherwise fall back to relative /api
// (works via Vite proxy in dev, same-origin in production)
const API_BASE = import.meta.env.VITE_API_BASE || '/api'

let refreshPromise: Promise<string | null> | null = null

export async function refreshAccessToken(): Promise<string | null> {
  const { refreshToken } = useAuthStore.getState()
  if (!refreshToken) return null

  // Deduplicate concurrent refresh calls
  if (refreshPromise) return refreshPromise

  refreshPromise = (async () => {
    try {
      const response = await fetch(`${API_BASE}/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: refreshToken }),
      })

      if (!response.ok) {
        useAuthStore.getState().logout()
        return null
      }

      const data = await response.json()
      useAuthStore.getState().setTokens({
        access_token: data.access_token,
        refresh_token: data.refresh_token ?? refreshToken,
      })
      return data.access_token
    } catch {
      useAuthStore.getState().logout()
      return null
    } finally {
      refreshPromise = null
    }
  })()

  return refreshPromise
}

export async function apiClient<T>(
  endpoint: string,
  options: RequestInit = {},
): Promise<T> {
  const store = useAuthStore.getState()

  // If the token is expired, try refreshing before the request
  let token = store.token
  if (token && store.isTokenExpired()) {
    const newToken = await refreshAccessToken()
    if (!newToken) {
      throw new ApiClientError('UNAUTHORIZED', 'Session expired. Please log in again.', 401)
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

  const response = await fetch(`${API_BASE}${endpoint}`, {
    ...options,
    headers,
  })

  // On 401, try refreshing the token once and retry
  if (response.status === 401) {
    const newToken = await refreshAccessToken()
    if (newToken) {
      headers['Authorization'] = `Bearer ${newToken}`
      const retryResponse = await fetch(`${API_BASE}${endpoint}`, {
        ...options,
        headers,
      })

      if (!retryResponse.ok) {
        const error = await retryResponse.json().catch(() => ({}))
        throw new ApiClientError(
          error.code ?? error.error?.code ?? 'ERROR',
          error.message ?? error.error?.message ?? 'Request failed',
          retryResponse.status,
        )
      }

      return retryResponse.json()
    }

    store.logout()
    throw new ApiClientError('UNAUTHORIZED', 'Session expired. Please log in again.', 401)
  }

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    const code = error.code ?? error.error?.code ?? 'ERROR'
    const message = error.message ?? error.error?.message ?? 'Request failed'
    throw new ApiClientError(code, message, response.status)
  }

  return response.json()
}

export class ApiClientError extends Error {
  code: string
  status: number

  constructor(code: string, message: string, status: number) {
    super(message)
    this.code = code
    this.status = status
    this.name = 'ApiClientError'
  }
}
