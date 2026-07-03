import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement, type ReactNode } from 'react'
import { useUploadFile } from './files'
import { useAuthStore } from '../stores/authStore'

// Mock apiClient so we can assert it is the only HTTP path used by the hook.
const apiClientMock = vi.fn()
vi.mock('./client', () => ({
  apiClient: (...args: unknown[]) => apiClientMock(...args),
  ApiClientError: class ApiClientError extends Error {
    code: string
    status: number
    constructor(code: string, message: string, status: number) {
      super(message)
      this.code = code
      this.status = status
      this.name = 'ApiClientError'
    }
  },
}))

// global.fetch must NOT be called by the hook anymore.
const fetchSpy = vi.fn()
vi.stubGlobal('fetch', fetchSpy)

function setToken(token: string | null) {
  useAuthStore.setState({ token, isAuthenticated: token !== null })
}

function wrapWithQueryClient() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: queryClient }, children)
  return { wrapper, queryClient }
}

describe('useUploadFile', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setToken('test-access-token')
  })

  afterEach(() => {
    setToken(null)
  })

  it('routes the upload through apiClient with FormData body and POST method', async () => {
    // Given: apiClient resolves an UploadResponse
    apiClientMock.mockResolvedValueOnce({
      file_id: 'f1',
      url: 'http://localhost/files/f1',
      original_name: 'a.png',
      size: 1,
      mime_type: 'image/png',
    })

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useUploadFile(), { wrapper })

    // When: trigger the mutation with a File
    const file = new File(['x'], 'a.png', { type: 'image/png' })
    await result.current.mutateAsync(file)

    // Then: apiClient was called (NOT global.fetch) with the right endpoint/body
    expect(fetchSpy).not.toHaveBeenCalled()
    expect(apiClientMock).toHaveBeenCalledTimes(1)
    const [endpoint, options] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/files/upload')
    expect((options as RequestInit).method).toBe('POST')
    expect((options as RequestInit).body).toBeInstanceOf(FormData)
    expect(((options as RequestInit).body as FormData).get('file')).toBe(file)
  })

  it('does NOT hardcode /api/files/upload anywhere (uses apiClient API_BASE)', async () => {
    apiClientMock.mockResolvedValueOnce({
      file_id: 'f1',
      url: 'http://localhost/files/f1',
      original_name: 'a.png',
      size: 1,
      mime_type: 'image/png',
    })

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useUploadFile(), { wrapper })

    await result.current.mutateAsync(new File(['x'], 'a.png', { type: 'image/png' }))

    // The endpoint passed to apiClient must be the relative '/files/upload'
    // (NOT an absolute '/api/files/upload' URL — apiClient prepends API_BASE).
    const [endpoint] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/files/upload')
    expect(endpoint).not.toContain('/api/files/upload')
  })

  it('rethrows ApiClientError on non-2xx so callers can catch it', async () => {
    // Given: apiClient rejects (it does the res.ok check internally and throws)
    const err = new (class TestErr extends Error {
      code = 'UPLOAD_TOO_LARGE'
      status = 413
      name = 'ApiClientError'
    })('Upload failed')
    apiClientMock.mockRejectedValueOnce(err)

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useUploadFile(), { wrapper })

    // When/Then: mutateAsync rejects with the same error
    await waitFor(() => expect(result.current.mutate).toBeDefined())
    await expect(
      result.current.mutateAsync(new File(['x'], 'a.png', { type: 'image/png' })),
    ).rejects.toThrow('Upload failed')
  })
})
