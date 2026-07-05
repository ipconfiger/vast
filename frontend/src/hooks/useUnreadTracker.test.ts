import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { cleanup, renderHook } from '@testing-library/react'

// ── Module mocks ──────────────────────────────────────────────────────
//
// The hook pulls four collaborators: the WS manager (subscribe API),
// react-router (useParams), and two stores (auth + unread). Each is
// replaced with a controllable stub so the assertions never touch real
// state or sockets.

type MsgListener = (data: unknown) => void

// Mutable bag of state read by the mocked useParams / getState.
interface TestHarness {
  currentChannelId: string | undefined
  myId: string | undefined
  increment: ReturnType<typeof vi.fn>
  clear: ReturnType<typeof vi.fn>
  subscribe: ReturnType<typeof vi.fn>
  lastUnsub: ReturnType<typeof vi.fn>
}

let harness: TestHarness

vi.mock('./useWebSocket', () => ({
  getWsManager: () => ({
    subscribe: (...args: unknown[]) => harness.subscribe(...args),
  }),
}))

vi.mock('../stores/unreadStore', () => ({
  useUnreadStore: {
    getState: () => ({
      increment: (...args: unknown[]) => harness.increment(...args),
      clear: (...args: unknown[]) => harness.clear(...args),
    }),
  },
}))

vi.mock('../stores/authStore', () => ({
  useAuthStore: {
    getState: () => ({ user: harness.myId ? { id: harness.myId } : null }),
  },
}))

vi.mock('react-router', () => ({
  useParams: () => ({ channelId: harness.currentChannelId }),
}))

// Import AFTER mocks so the hook sees the stubbed dependencies.
import { useUnreadTracker } from './useUnreadTracker'

describe('useUnreadTracker', () => {
  beforeEach(() => {
    harness = {
      currentChannelId: 'ch-current',
      myId: 'u-me',
      increment: vi.fn(),
      clear: vi.fn(),
      subscribe: vi.fn(),
      lastUnsub: vi.fn(),
    }
    // Default subscribe impl: capture the listener, return the unsub spy.
    harness.subscribe.mockImplementation((_type: string, _cb: MsgListener) => {
      return harness.lastUnsub
    })
  })

  afterEach(() => {
    cleanup()
  })

  it('Given a new_msg from another user on a non-current channel, When the event fires, Then increment is called with that channelId', () => {
    // Given
    let captured: MsgListener | null = null
    harness.subscribe.mockImplementation((_type: string, cb: MsgListener) => {
      captured = cb
      return harness.lastUnsub
    })
    renderHook(() => useUnreadTracker())
    expect(captured).not.toBeNull()

    // When: another user posts in channel ch-other (not the current one)
    captured!({
      channel_id: 'ch-other',
      sender_id: 'u-someone-else',
    })

    // Then
    expect(harness.increment).toHaveBeenCalledTimes(1)
    expect(harness.increment).toHaveBeenCalledWith('ch-other')
  })

  it('Given a new_msg on the currently-viewed channel, When the event fires, Then increment is NOT called', () => {
    // Given: currentChannelId = 'ch-current' (from beforeEach)
    let captured: MsgListener | null = null
    harness.subscribe.mockImplementation((_type: string, cb: MsgListener) => {
      captured = cb
      return harness.lastUnsub
    })
    renderHook(() => useUnreadTracker())

    // When: a message arrives on the current channel from another user
    captured!({ channel_id: 'ch-current', sender_id: 'u-someone-else' })

    // Then
    expect(harness.increment).not.toHaveBeenCalled()
  })

  it('Given a new_msg sent by the current user (self-sent), When the event fires, Then increment is NOT called', () => {
    // Given: myId = 'u-me'
    let captured: MsgListener | null = null
    harness.subscribe.mockImplementation((_type: string, cb: MsgListener) => {
      captured = cb
      return harness.lastUnsub
    })
    renderHook(() => useUnreadTracker())

    // When: the sender is me, on a non-current channel
    captured!({ channel_id: 'ch-other', sender_id: 'u-me' })

    // Then: self-sent messages never increment any counter
    expect(harness.increment).not.toHaveBeenCalled()
  })

  it('Given the hook is mounted, When currentChannelId changes to a new channel, Then clear is called for that new channel', () => {
    // Given: starts on ch-current
    const { rerender } = renderHook(() => useUnreadTracker())
    // clear fires on initial mount too (currentChannelId is set), so account
    // for that baseline before driving the change-under-test.
    expect(harness.clear).toHaveBeenCalledWith('ch-current')
    harness.clear.mockClear()

    // When: user navigates to a different channel
    harness.currentChannelId = 'ch-next'
    rerender()

    // Then: the just-entered channel's unread count is cleared
    expect(harness.clear).toHaveBeenCalledTimes(1)
    expect(harness.clear).toHaveBeenCalledWith('ch-next')
  })

  it('Given the hook is mounted, When it unmounts, Then the WS subscription unsubscribe is called', () => {
    // Given
    const { unmount } = renderHook(() => useUnreadTracker())
    expect(harness.lastUnsub).not.toHaveBeenCalled()

    // When
    unmount()

    // Then: every subscribe() returned our unsub spy; at least one effect
    // cleanup ran (the WS subscription effect), proving cleanup is wired.
    expect(harness.lastUnsub).toHaveBeenCalled()
  })

  it('Given a malformed payload (no channel_id), When the event fires, Then increment is NOT called and no exception escapes the listener', () => {
    // Given
    let captured: MsgListener | null = null
    harness.subscribe.mockImplementation((_type: string, cb: MsgListener) => {
      captured = cb
      return harness.lastUnsub
    })
    renderHook(() => useUnreadTracker())

    // When: payload missing channel_id AND sender_id (should not throw)
    expect(() => captured!({ some_other_field: 'x' })).not.toThrow()
    expect(() => captured!(null)).not.toThrow()

    // Then
    expect(harness.increment).not.toHaveBeenCalled()
  })

  it('Given a payload with camelCase field names (defensive fallback), When the event fires, Then increment is called with that channelId', () => {
    // Given: backend sends snake_case, but the hook tolerates camelCase
    let captured: MsgListener | null = null
    harness.subscribe.mockImplementation((_type: string, cb: MsgListener) => {
      captured = cb
      return harness.lastUnsub
    })
    renderHook(() => useUnreadTracker())

    // When
    captured!({ channelId: 'ch-camel', senderId: 'u-other' })

    // Then
    expect(harness.increment).toHaveBeenCalledWith('ch-camel')
  })
})
