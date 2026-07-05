import { describe, it, expect, beforeEach } from 'vitest'
import { useUnreadStore } from './unreadStore'

describe('unreadStore', () => {
  beforeEach(() => {
    useUnreadStore.getState().clearAll()
  })

  it('increments a new channel to 1', () => {
    useUnreadStore.getState().increment('ch-1')
    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBe(1)
  })

  it('increments an existing channel accumulates', () => {
    useUnreadStore.getState().increment('ch-1')
    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBe(1)

    useUnreadStore.getState().increment('ch-1')
    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBe(2)

    useUnreadStore.getState().increment('ch-1')
    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBe(3)
  })

  it('increments different channels independently', () => {
    useUnreadStore.getState().increment('ch-1')
    useUnreadStore.getState().increment('ch-2')
    useUnreadStore.getState().increment('ch-1')

    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBe(2)
    expect(useUnreadStore.getState().unreadByChannel['ch-2']).toBe(1)
  })

  it('clear removes the key entirely (entry is undefined after, NOT 0)', () => {
    useUnreadStore.getState().increment('ch-1')
    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBe(1)

    useUnreadStore.getState().clear('ch-1')
    expect(useUnreadStore.getState().unreadByChannel['ch-1']).toBeUndefined()
  })

  it('clear on a non-existent key is a no-op', () => {
    const initialState = useUnreadStore.getState().unreadByChannel

    useUnreadStore.getState().clear('ch-999')

    expect(useUnreadStore.getState().unreadByChannel).toEqual(initialState)
  })

  it('clearAll empties the entire map', () => {
    useUnreadStore.getState().increment('ch-1')
    useUnreadStore.getState().increment('ch-2')
    useUnreadStore.getState().increment('ch-3')

    expect(Object.keys(useUnreadStore.getState().unreadByChannel)).toHaveLength(3)

    useUnreadStore.getState().clearAll()

    expect(useUnreadStore.getState().unreadByChannel).toEqual({})
  })

  it('increment returns a new object, does not mutate the previous unreadByChannel', () => {
    const prevUnread = useUnreadStore.getState().unreadByChannel
    const prevRef = prevUnread

    useUnreadStore.getState().increment('ch-1')

    const currentUnread = useUnreadStore.getState().unreadByChannel

    expect(currentUnread).not.toBe(prevRef)
    expect(prevUnread).toEqual({})
    expect(currentUnread).toEqual({ 'ch-1': 1 })
  })
})