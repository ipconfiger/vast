import { useEffect, useRef } from 'react'
import { getWsManager } from './useWebSocket'
import { useChannelStore } from '../stores/channelStore'
import { useMessageStore } from '../stores/messageStore'
import { apiClient } from '../api/client'
import type { Message } from '../types'

/**
 * React hook that keeps messages in sync with the server via cursor-based
 * pagination. It:
 *
 * 1. Fetches messages (optionally after a stored cursor) on channel switch.
 * 2. Listens for `new_msg` WS events and re-fetches for the active channel.
 * 3. Re-fetches on WebSocket reconnect to catch missed messages.
 * 4. Never clears messages on disconnect — only appends/dedups.
 */
export function useCursorSync(): void {
  const currentChannelId = useChannelStore((s) => s.currentChannelId)
  const manager = getWsManager()

  // Guard against concurrent fetches (per-channel)
  const fetchingRef = useRef<Set<string>>(new Set())

  useEffect(() => {
    if (!currentChannelId) return

    const fetchAfterCursor = async (channelId: string): Promise<void> => {
      if (fetchingRef.current.has(channelId)) return
      fetchingRef.current.add(channelId)

      try {
        const cursor =
          useMessageStore.getState().lastCursorByChannel.get(channelId) ?? null
        const endpoint = cursor
          ? `/channels/${channelId}/messages?cursor=${encodeURIComponent(cursor)}`
          : `/channels/${channelId}/messages`
        const data = await apiClient<{ messages: Message[]; next_cursor: number; has_more: boolean }>(endpoint)
        const messages = data.messages
        if (messages.length === 0) return

        // Merge with existing messages, dedup by msg_id
        const state = useMessageStore.getState()
        const existing = state.messagesByChannel.get(channelId) ?? []
        const existingIds = new Set(existing.map((m) => m.msg_id))
        const newMsgs = messages.filter((m) => !existingIds.has(m.msg_id))

        if (newMsgs.length > 0) {
          const all = [...existing, ...newMsgs]
          state.setMessages(channelId, all, all[all.length - 1].msg_id)
        } else {
          // No new unique messages but still advance the cursor from the
          // server's latest response to avoid re-fetching the same batch.
          state.setMessages(channelId, existing, messages[messages.length - 1].msg_id)
        }
      } finally {
        fetchingRef.current.delete(channelId)
      }
    }

    // ── 1. Initial / channel-switch fetch ──────────────────────────
    fetchAfterCursor(currentChannelId)

    // ── 2. Subscribe to new_msg WS events ──────────────────────────
    const unsubMsg = manager.subscribe('new_msg', (data: unknown) => {
      const payload = data as Record<string, unknown> | null
      if (!payload) return
      const msgChannelId = (payload.channel_id ?? payload.channelId) as
        | string
        | undefined
      if (msgChannelId === currentChannelId) {
        fetchAfterCursor(currentChannelId)
      }
    })

    // ── 3. Reconnect refetch ───────────────────────────────────────
    const unsubReconnect = manager.onReconnect(() => {
      if (currentChannelId) {
        fetchAfterCursor(currentChannelId)
      }
    })

    return () => {
      unsubMsg()
      unsubReconnect()
    }
  }, [currentChannelId, manager])
}
