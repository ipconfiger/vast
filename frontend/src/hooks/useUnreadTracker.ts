import { useEffect } from 'react'
import { useParams } from 'react-router'
import { getWsManager } from './useWebSocket'
import { useUnreadStore } from '../stores/unreadStore'
import { useAuthStore } from '../stores/authStore'

/**
 * Tracks unread messages globally: subscribes to WS `new_msg` events once
 * per `currentChannelId` change, increments the unread counter for any
 * non-current, non-self message, and clears the counter for the channel
 * the user just entered. Mount ONCE at the AppLayout level.
 *
 * The WS subscription lives in an effect keyed on `currentChannelId` so the
 * callback always closes over the fresh value — an empty-deps effect would
 * capture a stale `currentChannelId` and break the skip-current rule.
 */
export function useUnreadTracker(): void {
  const { channelId: currentChannelId } = useParams<{ channelId?: string }>()
  const manager = getWsManager()

  useEffect(() => {
    const unsub = manager.subscribe('new_msg', (data: unknown) => {
      const payload = data as Record<string, unknown> | null
      if (!payload) return

      // Payload fields are snake_case (serde rename_all). Defensive read
      // mirrors useCursorSync so a stray camelCase key still works.
      const msgChannelId = (payload.channel_id ?? payload.channelId) as
        | string
        | undefined
      const senderId = (payload.sender_id ?? payload.senderId) as
        | string
        | undefined

      if (!msgChannelId) return
      if (msgChannelId === currentChannelId) return // viewing this channel
      const myId = useAuthStore.getState().user?.id
      if (senderId === myId) return // self-sent

      useUnreadStore.getState().increment(msgChannelId)
    })
    return unsub
  }, [manager, currentChannelId])

  // Clear unread count when entering a channel.
  useEffect(() => {
    if (currentChannelId) {
      useUnreadStore.getState().clear(currentChannelId)
    }
  }, [currentChannelId])
}
