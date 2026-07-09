import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement, type ReactNode } from 'react'
import { ChannelListPage } from './ChannelListPage'
import { useAuthStore } from '../stores/authStore'

// --- Mocks -----------------------------------------------------------------

// Mutable params so each test can set channelId.
const mockParams: { channelId?: string } = {}
const mockNavigate = vi.fn()
vi.mock('react-router', () => ({
  useParams: () => mockParams,
  useNavigate: () => mockNavigate,
  MemoryRouter: ({ children }: { children: ReactNode }) => children,
}))

// apiClient is the single funnel for useChannel.
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

// Silence WS and cursor-sync hooks in tests.
vi.mock('../hooks/useWebSocket', () => ({
  useWebSocket: () => ({ status: 'disconnected' as const }),
  getWsManager: () => ({
    subscribeChannel: () => {},
    subscribe: () => () => {},
    connect: () => {},
    disconnect: () => {},
    listenStatus: () => () => {},
  }),
}))
vi.mock('../hooks/useCursorSync', () => ({
  useCursorSync: () => {},
}))

// jsdom lacks URL.createObjectURL/revokeObjectURL.
function ensureUrlHelpers() {
  if (!(URL as unknown as { createObjectURL?: unknown }).createObjectURL) {
    ;(URL as unknown as { createObjectURL: () => string }).createObjectURL = () => 'blob:http://localhost/mock'
  }
  if (!(URL as unknown as { revokeObjectURL?: unknown }).revokeObjectURL) {
    ;(URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = () => {}
  }
}

function resetStores() {
  useAuthStore.setState({
    token: 'test-token',
    refreshToken: null,
    tokenExpiry: Date.now() + 3600_000, // 1 hour in the future, so isTokenExpired() returns false
    user: { id: 'u1', username: 'alice', display_name: 'Alice', avatar_url: '', created_at: '' },
    isAuthenticated: true,
  })
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <>{createElement(ChannelListPage)}</>
    </QueryClientProvider>,
  )
}

function makeArchivedChannel(overrides: Record<string, unknown> = {}) {
  return {
    id: 'archived-chan',
    name: 'old-project',
    type: 'public' as const,
    created_by: 'u1',
    created_at: '2024-01-01T00:00:00Z',
    is_archived: true,
    owner_id: 'u1',
    ...overrides,
  }
}

function makeActiveChannel(overrides: Record<string, unknown> = {}) {
  return {
    id: 'active-chan',
    name: 'general',
    type: 'public' as const,
    created_by: 'u1',
    created_at: '2024-01-01T00:00:00Z',
    is_archived: false,
    owner_id: 'u1',
    ...overrides,
  }
}

// --- Tests -----------------------------------------------------------------

describe('ChannelListPage - archived channel view', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ensureUrlHelpers()
    resetStores()
    mockParams.channelId = ''
    mockNavigate.mockReset()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders download prompt for archived channel (not chat UI)', async () => {
    mockParams.channelId = 'archived-chan'
    apiClientMock.mockImplementation((endpoint: string) => {
      if (endpoint === '/channels/archived-chan') {
        return Promise.resolve(makeArchivedChannel())
      }
      return Promise.resolve([])
    })

    const { findByText, container } = renderPage()

    // Wait for the archived message to appear
    expect(await findByText('This channel has been archived.')).toBeTruthy()
    expect(await findByText('Download Archive')).toBeTruthy()
    expect(await findByText('Back to Channels')).toBeTruthy()
    expect(await findByText('[Archived]')).toBeTruthy()

    // Chat UI should NOT be rendered
    // The archived container is a flex-col with items-center justify-center;
    // the normal chat has a ChannelHeader with border-b. Check that we see
    // the archive heading text _plus_ that MessageInput isn't present.
    const messageInput = container.querySelector('[class*="message-input"]')
    expect(messageInput).toBeNull()
  })

  it('renders normal chat UI for active channel', async () => {
    mockParams.channelId = 'active-chan'
    apiClientMock.mockImplementation((endpoint: string) => {
      if (endpoint === '/channels/active-chan') {
        return Promise.resolve(makeActiveChannel())
      }
      return Promise.resolve([])
    })

    const { findByText, queryByText } = renderPage()

    // ChannelHeader shows channel name
    expect(await findByText(/general/)).toBeTruthy()

    // Archived elements should NOT appear
    expect(queryByText('This channel has been archived.')).toBeNull()
    expect(queryByText('Download Archive')).toBeNull()
  })

  it('triggers download when Download Archive is clicked', async () => {
    mockParams.channelId = 'archived-chan'
    apiClientMock.mockImplementation((endpoint: string) => {
      if (endpoint === '/channels/archived-chan') {
        return Promise.resolve(makeArchivedChannel())
      }
      return Promise.resolve([])
    })

    // Stub fetch so downloadChannelArchive doesn't make a real request
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      blob: () => Promise.resolve(new Blob()),
    })
    vi.stubGlobal('fetch', fetchMock)

    const { findByText } = renderPage()
    const btn = await findByText('Download Archive')
    fireEvent.click(btn)

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining('/channels/archived-chan/archive/download'),
        expect.objectContaining({ headers: expect.objectContaining({ Authorization: 'Bearer test-token' }) }),
      )
    })

    vi.unstubAllGlobals()
  })

  it('navigates to channels list on Back to Channels click', async () => {
    mockParams.channelId = 'archived-chan'
    apiClientMock.mockImplementation((endpoint: string) => {
      if (endpoint === '/channels/archived-chan') {
        return Promise.resolve(makeArchivedChannel())
      }
      return Promise.resolve([])
    })

    const { findByText } = renderPage()
    const btn = await findByText('Back to Channels')
    fireEvent.click(btn)

    expect(mockNavigate).toHaveBeenCalledWith('/channels')
  })
})
