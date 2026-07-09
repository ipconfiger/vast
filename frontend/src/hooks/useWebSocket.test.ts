import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { cleanup, renderHook } from '@testing-library/react'
import { useWebSocket, getWsManager } from './useWebSocket'
import { useAuthStore } from '../stores/authStore'
import { useChannelStore } from '../stores/channelStore'
import { useToastStore } from '../stores/toastStore'
import { queryClient } from '../queryClient'
import type { User } from '../types'

// Mock queryClient so refetch/invalidate calls are observable and never touch the network.
vi.mock('../queryClient', () => ({
  queryClient: {
    invalidateQueries: vi.fn(),
    refetchQueries: vi.fn(),
  },
}))

// Mock refreshAccessToken so the reconnect path cannot reach the network.
vi.mock('../api/client', () => ({
  refreshAccessToken: vi.fn().mockResolvedValue(undefined),
}))

// Cast the mocked queryClient to its spy shape for assertions.
const qc = queryClient as unknown as {
  invalidateQueries: ReturnType<typeof vi.fn>
  refetchQueries: ReturnType<typeof vi.fn>
}

// ── Fake WebSocket ──────────────────────────────────────────────────────
// The hook constructs `new WebSocket(url)`; we replace the global class so
// each instance is captured and its onopen/onmessage callbacks driven by
// the test instead of a real socket.

interface CapturedWs {
  url: string
  onopen: (() => void) | null
  onmessage: ((ev: { data: string }) => void) | null
  onclose: (() => void) | null
  onerror: (() => void) | null
  close: () => void
  send: (data: string) => void
}

const captured: CapturedWs[] = []

class FakeWebSocketImpl {
  onopen: (() => void) | null = null
  onmessage: ((ev: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  constructor(public url: string) {
    captured.push(this)
  }
  close() {}
  send() {}
}

function openLastWs(): void {
  const ws = captured[captured.length - 1]
  if (!ws?.onopen) throw new Error('WS not connected: onopen missing')
  ws.onopen()
}

function emitServerEvent(type: string, data: unknown): void {
  const ws = captured[captured.length - 1]
  if (!ws?.onmessage) throw new Error('WS not connected: onmessage missing')
  ws.onmessage({ data: JSON.stringify({ type, data }) })
}

// ── Fixtures ────────────────────────────────────────────────────────────

const ME: User = {
  id: 'u-me',
  username: 'me',
  display_name: 'Me',
  created_at: '',
}

describe('useWebSocket — member_added dedup + toastStore routing', () => {
  let originalWebSocket: typeof WebSocket

  beforeEach(() => {
    // Fake timers prevent the member_added reload setTimeout(1500) from firing
    // in jsdom (where window.location.reload is non-writable) and keep the
    // toast auto-remove timers from racing the synchronous assertions.
    vi.useFakeTimers()

    originalWebSocket = globalThis.WebSocket
    ;(globalThis as unknown as { WebSocket: unknown }).WebSocket = FakeWebSocketImpl
    captured.length = 0

    // Seed auth store so the hook treats the user as authenticated.
    useAuthStore.setState({
      token: 'tok',
      refreshToken: 'rtok',
      tokenExpiry: null,
      user: ME,
      isAuthenticated: true,
    })
    useToastStore.setState({ toasts: [] })

    qc.invalidateQueries.mockClear()
    qc.refetchQueries.mockClear()
  })

  afterEach(() => {
    cleanup()
    // Reset the singleton manager so the next test's connect() creates a fresh WS.
    getWsManager().disconnect()
    ;(globalThis as unknown as { WebSocket: unknown }).WebSocket = originalWebSocket
    vi.useRealTimers()
  })

  it('Given an authenticated WS connection, When member_added targets my user_id, Then exactly ONE toast is added to the store and refetchQueries fires once per query', () => {
    // Given
    renderHook(() => useWebSocket())
    openLastWs()

    // When
    emitServerEvent('member_added', {
      channel_id: 'ch-1',
      user_id: ME.id,
      username: ME.username,
    })

    // Then: dedup guarantee — a single event yields a single toast.
    expect(useToastStore.getState().toasts).toHaveLength(1)
    expect(useToastStore.getState().toasts[0].type).toBe('info')

    const channelsRefetches = qc.refetchQueries.mock.calls.filter(
      (c) => c[0]?.queryKey?.[0] === 'channels',
    )
    expect(channelsRefetches).toHaveLength(1)
  })

  it('Given an authenticated WS connection, When member_added targets another user, Then NO toast is added and NO refetch or reload happens', () => {
    // Given
    renderHook(() => useWebSocket())
    openLastWs()

    // When
    emitServerEvent('member_added', {
      channel_id: 'ch-2',
      user_id: 'someone-else',
      username: 'other',
    })

    // Then
    expect(useToastStore.getState().toasts).toHaveLength(0)
    expect(qc.refetchQueries).not.toHaveBeenCalled()
  })
})

describe('useWebSocket — channel_archived / channel_unarchived', () => {
  let originalWebSocket: typeof WebSocket

  beforeEach(() => {
    vi.useFakeTimers()
    originalWebSocket = globalThis.WebSocket
    ;(globalThis as unknown as { WebSocket: unknown }).WebSocket = FakeWebSocketImpl
    captured.length = 0
    useAuthStore.setState({
      token: 'tok',
      refreshToken: 'rtok',
      tokenExpiry: null,
      user: ME,
      isAuthenticated: true,
    })
    useChannelStore.setState({
      channels: [
        { id: 'ch-1', name: 'general', is_archived: false, created_at: '' },
        { id: 'ch-2', name: 'archive-me', is_archived: false, created_at: '' },
      ] as any,
      currentChannelId: null,
    })
    qc.invalidateQueries.mockClear()
    qc.refetchQueries.mockClear()
  })

  afterEach(() => {
    cleanup()
    getWsManager().disconnect()
    ;(globalThis as unknown as { WebSocket: unknown }).WebSocket = originalWebSocket
    vi.useRealTimers()
  })

  it('Given an authenticated WS connection, When channel_archived is received, Then updateChannel is called with is_archived: true', () => {
    renderHook(() => useWebSocket())
    openLastWs()
    const spy = vi.spyOn(useChannelStore.getState(), 'updateChannel')
    emitServerEvent('channel_archived', { channel_id: 'ch-2' })
    expect(spy).toHaveBeenCalledWith('ch-2', { is_archived: true })
  })

  it('Given currentChannelId matches the archived channel, When channel_archived is received, Then updateChannel is called and queryClient invalidates channel and channels queries', () => {
    useChannelStore.setState({ currentChannelId: 'ch-2' })
    renderHook(() => useWebSocket())
    openLastWs()
    emitServerEvent('channel_archived', { channel_id: 'ch-2' })
    // currentChannelId is NOT reset to null — the handler leaves store state
    // as-is and relies on queryClient invalidation to drive the UI updates.
    expect(useChannelStore.getState().currentChannelId).toBe('ch-2')
    expect(qc.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['channel', 'ch-2'] })
    expect(qc.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['channels'] })
  })

  it('Given an authenticated WS connection, When channel_unarchived is received, Then updateChannel is called with is_archived: false', () => {
    renderHook(() => useWebSocket())
    openLastWs()
    const spy = vi.spyOn(useChannelStore.getState(), 'updateChannel')
    emitServerEvent('channel_unarchived', { channel_id: 'ch-1' })
    expect(spy).toHaveBeenCalledWith('ch-1', { is_archived: false })
  })

  it('Given an invalid channel_id in the message, When handler fires, Then no store mutation occurs', () => {
    renderHook(() => useWebSocket())
    openLastWs()
    const spy = vi.spyOn(useChannelStore.getState(), 'updateChannel')
    emitServerEvent('channel_archived', { channel_id: null })
    expect(spy).not.toHaveBeenCalled()
  })
})
