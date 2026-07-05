import { describe, it, expect, beforeEach, vi } from 'vitest'
import { apiClient } from './client'

vi.mock('../stores/authStore', () => ({
  useAuthStore: {
    getState: vi.fn(() => ({
      token: 'test-access-token',
      refreshToken: 'test-refresh-token',
      isTokenExpired: vi.fn(() => false),
      logout: vi.fn(),
    })),
  },
}))

const mockFetch = vi.fn()
globalThis.fetch = mockFetch

describe('apiClient', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should not send Content-Type: application/json for FormData body', async () => {
    const formData = new FormData()
    formData.append('file', new Blob(['test']), 'test.txt')

    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ success: true }),
    })

    await apiClient('/test', { method: 'POST', body: formData })

    expect(mockFetch).toHaveBeenCalledTimes(1)
    const fetchCall = mockFetch.mock.calls[0]
    const headers = fetchCall[1]?.headers as Record<string, string>

    // FormData should NOT have Content-Type: application/json
    expect(headers).toBeDefined()
    expect(headers['Content-Type']).toBeUndefined()

    // But it SHOULD have Authorization
    expect(headers['Authorization']).toBe('Bearer test-access-token')
  })

  it('should send Content-Type: application/json for JSON body (regression guard)', async () => {
    const jsonBody = JSON.stringify({ a: 1 })

    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ success: true }),
    })

    await apiClient('/test', { method: 'POST', body: jsonBody })

    expect(mockFetch).toHaveBeenCalledTimes(1)
    const fetchCall = mockFetch.mock.calls[0]
    const headers = fetchCall[1]?.headers as Record<string, string>

    // JSON body SHOULD have Content-Type: application/json
    expect(headers).toBeDefined()
    expect(headers['Content-Type']).toBe('application/json')

    // And it SHOULD have Authorization
    expect(headers['Authorization']).toBe('Bearer test-access-token')
  })
})