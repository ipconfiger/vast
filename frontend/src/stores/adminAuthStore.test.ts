import { describe, it, expect, beforeEach } from 'vitest'
import { useAdminAuthStore } from './adminAuthStore'

// Reset to logged-out baseline before each test. persist middleware
// reads/writes localStorage; clearing the key + setting initial state
// isolates every test from prior runs.
beforeEach(() => {
  localStorage.removeItem('admin-auth-storage')
  useAdminAuthStore.setState({
    adminToken: null,
    adminRefreshToken: null,
    adminTokenExpiry: null,
    isAuthenticated: false,
    username: null,
  })
})

describe('adminAuthStore.login', () => {
  it('stores tokens, computes expiry, and authenticates', () => {
    const before = Date.now()
    useAdminAuthStore.getState().login(
      {
        access_token: 'access-123',
        refresh_token: 'refresh-456',
        expires_in: 3600,
      },
      'admin',
    )
    const after = Date.now()

    const s = useAdminAuthStore.getState()
    expect(s.adminToken).toBe('access-123')
    expect(s.adminRefreshToken).toBe('refresh-456')
    expect(s.username).toBe('admin')
    expect(s.isAuthenticated).toBe(true)
    // expires_in is seconds → expiry is now + 3600 * 1000ms
    expect(s.adminTokenExpiry).toBeGreaterThanOrEqual(before + 3600 * 1000)
    expect(s.adminTokenExpiry).toBeLessThanOrEqual(after + 3600 * 1000)
  })
})

describe('adminAuthStore.logout', () => {
  it('clears all fields', () => {
    useAdminAuthStore.getState().login(
      { access_token: 'a', refresh_token: 'r', expires_in: 60 },
      'admin',
    )
    expect(useAdminAuthStore.getState().isAuthenticated).toBe(true)

    useAdminAuthStore.getState().logout()

    const s = useAdminAuthStore.getState()
    expect(s.adminToken).toBeNull()
    expect(s.adminRefreshToken).toBeNull()
    expect(s.adminTokenExpiry).toBeNull()
    expect(s.username).toBeNull()
    expect(s.isAuthenticated).toBe(false)
  })

  it('persists cleared state so a reload cannot restore a session', () => {
    useAdminAuthStore.getState().login(
      { access_token: 'a', refresh_token: 'r', expires_in: 60 },
      'admin',
    )
    expect(localStorage.getItem('admin-auth-storage')).not.toBeNull()

    useAdminAuthStore.getState().logout()

    // persist re-writes the cleared (null) state; the security invariant
    // is that no token survives a reload, not that the key is absent.
    const raw = localStorage.getItem('admin-auth-storage')
    expect(raw).not.toBeNull()
    const parsed = JSON.parse(raw as string).state
    expect(parsed.adminToken).toBeNull()
    expect(parsed.adminRefreshToken).toBeNull()
    expect(parsed.username).toBeNull()
  })
})

describe('adminAuthStore.setTokens', () => {
  it('updates tokens without clearing username', () => {
    useAdminAuthStore.getState().login(
      { access_token: 'old-access', refresh_token: 'old-refresh', expires_in: 60 },
      'root',
    )

    useAdminAuthStore.getState().setTokens({
      access_token: 'new-access',
      refresh_token: 'new-refresh',
      expires_in: 120,
    })

    const s = useAdminAuthStore.getState()
    expect(s.adminToken).toBe('new-access')
    expect(s.adminRefreshToken).toBe('new-refresh')
    expect(s.username).toBe('root') // preserved
    expect(s.isAuthenticated).toBe(true)
  })
})

describe('adminAuthStore.isTokenExpired', () => {
  it('returns true when expiry is null', () => {
    expect(useAdminAuthStore.getState().isTokenExpired()).toBe(true)
  })

  it('returns true when expiry is in the past', () => {
    useAdminAuthStore.setState({
      adminToken: 'a',
      adminTokenExpiry: Date.now() - 1000,
    })
    expect(useAdminAuthStore.getState().isTokenExpired()).toBe(true)
  })

  it('returns true inside the 30s buffer window', () => {
    // 15s before expiry → within 30s buffer → expired
    useAdminAuthStore.setState({
      adminToken: 'a',
      adminTokenExpiry: Date.now() + 15_000,
    })
    expect(useAdminAuthStore.getState().isTokenExpired()).toBe(true)
  })

  it('returns false when expiry is comfortably in the future', () => {
    useAdminAuthStore.setState({
      adminToken: 'a',
      adminTokenExpiry: Date.now() + 3_600_000,
    })
    expect(useAdminAuthStore.getState().isTokenExpired()).toBe(false)
  })
})

describe('adminAuthStore persistence', () => {
  it('persists only data fields, derives isAuthenticated on rehydrate', () => {
    useAdminAuthStore.getState().login(
      { access_token: 'persist-access', refresh_token: 'persist-refresh', expires_in: 3600 },
      'persisted-admin',
    )

    const raw = localStorage.getItem('admin-auth-storage')
    expect(raw).not.toBeNull()
    const parsed = JSON.parse(raw as string).state
    expect(parsed.adminToken).toBe('persist-access')
    expect(parsed.adminRefreshToken).toBe('persist-refresh')
    expect(parsed.username).toBe('persisted-admin')
    expect(parsed.adminTokenExpiry).toBeTypeOf('number')
    // isAuthenticated must NOT be persisted (partialize omits it)
    expect(parsed.isAuthenticated).toBeUndefined()
  })
})
