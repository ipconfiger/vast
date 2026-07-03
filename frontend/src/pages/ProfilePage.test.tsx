import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router'
import { createElement, type ReactNode } from 'react'
import ProfilePage from './ProfilePage'
import { useAuthStore } from '../stores/authStore'
import { useToastStore } from '../stores/toastStore'

// --- Mocks -----------------------------------------------------------------

// apiClient is the single funnel for both files.ts (useUploadFile) and the
// page's own PATCH /auth/profile call. We mock it once and switch behavior
// per-test on apiClientMock.mockImplementation.
const apiClientMock = vi.fn()
vi.mock('../api/client', () => ({
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

const mockNavigate = vi.fn()
vi.mock('react-router', () => ({
  useNavigate: () => mockNavigate,
  MemoryRouter: ({ children }: { children: ReactNode }) => children,
}))

// jsdom lacks URL.createObjectURL/revokeObjectURL; stub them so the avatar
// effect (still active from T4) doesn't crash if it ever fires.
function ensureUrlHelpers() {
  if (!(URL as unknown as { createObjectURL?: unknown }).createObjectURL) {
    ;(URL as unknown as { createObjectURL: () => string }).createObjectURL = () => 'blob:http://localhost/mock'
  }
  if (!(URL as unknown as { revokeObjectURL?: unknown }).revokeObjectURL) {
    ;(URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = () => {}
  }
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:http://localhost/mock')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
}

// --- Helpers ---------------------------------------------------------------

function resetStores() {
  useAuthStore.setState({
    token: 'test-access-token',
    refreshToken: 'test-refresh-token',
    tokenExpiry: Date.now() + 3_600_000,
    user: {
      id: 'u1',
      username: 'alice',
      display_name: 'Alice',
      avatar_url: '',
      created_at: '',
    },
    isAuthenticated: true,
  })
  useToastStore.setState({ toasts: [] })
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{createElement(ProfilePage)}</MemoryRouter>
    </QueryClientProvider>,
  )
}

async function waitFetched() {
  // The page sets `fetched` only after the GET /auth/profile resolves.
  await waitFor(() => {
    expect(apiClientMock).toHaveBeenCalledWith('/auth/profile')
  })
}

async function pickFile(container: HTMLElement, filename = 'avatar.png') {
  const input = container.querySelector('input[type="file"]') as HTMLInputElement
  expect(input).toBeTruthy()
  const file = new File(['x'], filename, { type: 'image/png' })
  Object.defineProperty(input, 'files', { value: [file], configurable: true })
  fireEvent.change(input)
  // give the async handler a tick
  await Promise.resolve()
}

function errorToastMessages(): string[] {
  return useToastStore
    .getState()
    .toasts.filter((t) => t.type === 'error')
    .map((t) => t.message)
}

// --- Tests -----------------------------------------------------------------

describe('ProfilePage - upload error handling', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ensureUrlHelpers()
    resetStores()
  })

  afterEach(() => {
    vi.restoreAllMocks()
    useToastStore.setState({ toasts: [] })
  })

  it('surfaces toast.error("Upload failed: ...") and clears the spinner when POST /files/upload fails (500)', async () => {
    // Given: the page loads, then the upload POST rejects with ApiClientError
    apiClientMock.mockImplementation((endpoint: string) => {
      if (endpoint === '/auth/profile') {
        return Promise.resolve({
          id: 'u1',
          username: 'alice',
          display_name: 'Alice',
          avatar_url: '',
        })
      }
      if (endpoint === '/files/upload') {
        // Mimic what apiClient does internally on a 500: throw ApiClientError.
        const err = new (class extends Error {
          code = 'INTERNAL'
          status = 500
          name = 'ApiClientError'
        })('Upload failed')
        return Promise.reject(err)
      }
      return Promise.resolve({})
    })

    // When: render, wait for fetch, attach a file
    const { container } = renderPage()
    await waitFetched()
    await pickFile(container)

    // Then: an error toast with "Upload failed" surfaces
    await waitFor(() => {
      const msgs = errorToastMessages()
      expect(msgs.some((m) => /Upload failed/i.test(m))).toBe(true)
    })

    // And: no toast says "profile update failed" (the PATCH never ran)
    const msgs = errorToastMessages()
    expect(msgs.some((m) => /profile update failed/i.test(m))).toBe(false)

    // And: the spinner is gone (uploading state cleared in finally{})
    await waitFor(() => {
      expect(container.querySelector('.animate-spin.h-5')).toBeNull()
    })
  })

  it('shows "profile update failed" (NOT "Upload failed") when upload succeeds but PATCH /auth/profile fails', async () => {
    let patchCalled = false
    apiClientMock.mockImplementation((endpoint: string, options?: RequestInit) => {
      if (endpoint === '/auth/profile' && options?.method !== 'PATCH') {
        return Promise.resolve({
          id: 'u1',
          username: 'alice',
          display_name: 'Alice',
          avatar_url: '',
        })
      }
      if (endpoint === '/files/upload') {
        return Promise.resolve({
          file_id: 'f1',
          url: 'http://localhost/files/f1.png',
          original_name: 'a.png',
          size: 1,
          mime_type: 'image/png',
        })
      }
      if (endpoint === '/auth/profile' && options?.method === 'PATCH') {
        patchCalled = true
        const err = new (class extends Error {
          code = 'INTERNAL'
          status = 500
          name = 'ApiClientError'
        })('Profile update failed')
        return Promise.reject(err)
      }
      return Promise.resolve({})
    })

    const { container } = renderPage()
    await waitFetched()
    await pickFile(container)

    // Then: the PATCH was attempted and a "profile update failed" toast surfaces
    await waitFor(() => expect(patchCalled).toBe(true))
    await waitFor(() => {
      const msgs = errorToastMessages()
      expect(msgs.some((m) => /profile update failed/i.test(m))).toBe(true)
    })

    // And: NO toast says "Upload failed" (the upload itself succeeded)
    const msgs = errorToastMessages()
    expect(msgs.some((m) => /Upload failed/i.test(m))).toBe(false)

    // And: the spinner is cleared
    await waitFor(() => {
      expect(container.querySelector('.animate-spin.h-5')).toBeNull()
    })
  })
})
