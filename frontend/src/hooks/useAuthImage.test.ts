import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook } from '@testing-library/react'
import { useAuthImage } from './useAuthImage'
import { useAuthStore } from '../stores/authStore'

// Helpers ---------------------------------------------------------------

type FetchResult = { blob: () => Promise<Blob> } | Error

function mockFetchOnce(result: FetchResult): typeof fetch {
  const impl = vi.fn(async () => {
    if (result instanceof Error) throw result
    return result as unknown as Response
  })
  vi.stubGlobal('fetch', impl)
  return impl as unknown as typeof fetch
}

// jsdom lacks URL.createObjectURL/revokeObjectURL; define before spying.
function stubCreateObjectURL(returnValue = 'blob:http://localhost/mock-object-url') {
  if (!(URL as unknown as { createObjectURL?: unknown }).createObjectURL) {
    ;(URL as unknown as { createObjectURL: () => string }).createObjectURL = () => returnValue
  }
  return vi.spyOn(URL, 'createObjectURL').mockReturnValue(returnValue)
}

function stubRevokeObjectURL() {
  if (!(URL as unknown as { revokeObjectURL?: unknown }).revokeObjectURL) {
    ;(URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = () => {}
  }
  return vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
}

function stubAbortProto() {
  return vi.spyOn(AbortController.prototype, 'abort').mockImplementation(function (this: AbortController) {})
}

function setToken(token: string | null) {
  useAuthStore.setState({ token, isAuthenticated: token !== null })
}

// Tests -----------------------------------------------------------------

describe('useAuthImage', () => {
  let originalFetch: typeof fetch

  beforeEach(() => {
    originalFetch = globalThis.fetch
    setToken('test-token')
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
    globalThis.fetch = originalFetch
    useAuthStore.setState({ token: null, isAuthenticated: false })
  })

  it('calls URL.revokeObjectURL with the created URL and aborts the fetch on unmount', async () => {
    // Given: a fetch that resolves to a blob
    const fetchImpl = mockFetchOnce({ blob: async () => new Blob(['x'], { type: 'image/png' }) })
    const createSpy = stubCreateObjectURL('blob:http://localhost/abc-1')
    const revokeSpy = stubRevokeObjectURL()
    const abortSpy = stubAbortProto()

    // When: render the hook and let the effect fire
    const { result, unmount } = renderHook(() => useAuthImage('http://example.com/img.png'))

    // microtask flush so .then chains resolve
    await vi.waitFor(() => expect(createSpy).toHaveBeenCalledTimes(1))
    expect(result.current).toBe('blob:http://localhost/abc-1')
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://example.com/img.png',
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    )

    // Then: cleanup revokes the object URL and aborts the in-flight controller
    expect(revokeSpy).not.toHaveBeenCalled()
    expect(abortSpy).not.toHaveBeenCalled()
    unmount()
    expect(revokeSpy).toHaveBeenCalledTimes(1)
    expect(revokeSpy).toHaveBeenCalledWith('blob:http://localhost/abc-1')
    expect(abortSpy).toHaveBeenCalledTimes(1)
  })

  it('passes the bearer token header from the auth store', async () => {
    const fetchImpl = mockFetchOnce({ blob: async () => new Blob(['x']) })
    stubCreateObjectURL()
    stubRevokeObjectURL()
    stubAbortProto()

    renderHook(() => useAuthImage('http://example.com/img.png'))
    await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalled())
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://example.com/img.png',
      expect.objectContaining({
        headers: { Authorization: 'Bearer test-token' },
        signal: expect.any(AbortSignal),
      }),
    )
  })

  it('returns null and does not crash when fetch rejects before the blob is created', async () => {
    // Given: a fetch that rejects with a network error
    mockFetchOnce(new Error('network down'))
    const createSpy = stubCreateObjectURL()
    const revokeSpy = stubRevokeObjectURL()
    const abortSpy = stubAbortProto()

    // When: render and let the rejection propagate
    const { result, unmount } = renderHook(() => useAuthImage('http://example.com/img.png'))
    await vi.waitFor(() => expect(result.current).toBeNull())

    // Then: no object URL was created, no dangling blob, state is null
    expect(createSpy).not.toHaveBeenCalled()
    expect(revokeSpy).not.toHaveBeenCalled()

    // And: cleanup does not throw and does not invoke revoke (no URL to revoke)
    expect(() => unmount()).not.toThrow()
    expect(revokeSpy).not.toHaveBeenCalled()
    // abort is still called on cleanup (request was issued, must be cancellable)
    expect(abortSpy).toHaveBeenCalledTimes(1)
    expect(result.current).toBeNull()
  })

  it('returns null when no URL is provided and does not fetch', () => {
    const fetchImpl = mockFetchOnce({ blob: async () => new Blob(['x']) })
    const createSpy = stubCreateObjectURL()
    const abortSpy = stubAbortProto()

    const { result, unmount } = renderHook(() => useAuthImage(null))
    expect(result.current).toBeNull()
    expect(fetchImpl).not.toHaveBeenCalled()
    expect(createSpy).not.toHaveBeenCalled()
    unmount()
    expect(abortSpy).not.toHaveBeenCalled()
  })
})
