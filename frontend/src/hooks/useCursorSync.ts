import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { getWsManager } from './useWebSocket'
import { useChannelStore } from '../stores/channelStore'
import { useAuthStore } from '../stores/authStore'

export function useCursorSync(): void {
  const currentChannelId = useChannelStore((s) => s.currentChannelId)
  const manager = getWsManager()
  const queryClient = useQueryClient()

  useEffect(() => {
    if (!currentChannelId) return

    manager.subscribeChannel(currentChannelId)

    const unsubMsg = manager.subscribe('new_msg', (data: unknown) => {
      const payload = data as Record<string, unknown> | null
      if (!payload) return
      const msgChannelId = (payload.channel_id ?? payload.channelId) as string | undefined
      const senderId = (payload.sender_id ?? payload.senderId) as string | undefined
      const myId = useAuthStore.getState().user?.id
      // Skip when the current user sent this message — useSendMessage.onSuccess
      // already did addMessage + invalidateQueries; a second refetch is wasted.
      if (msgChannelId === currentChannelId && senderId !== myId) {
        queryClient.invalidateQueries({ queryKey: ['messages', currentChannelId] })
      }
    })

    // Message payload updated (e.g. join-request status changed) — refetch
    // the affected channel's messages. No senderId check: this is a system-
    // level update, not a user-sent message.
    const unsubMsgUpdated = manager.subscribe('msg_updated', (data: unknown) => {
      const payload = data as { channel_id?: string } | null
      if (!payload || typeof payload.channel_id !== 'string') return
      queryClient.invalidateQueries({ queryKey: ['messages', payload.channel_id] })
    })

    // Train queries are keyed by train_id, not channel — invalidate
    // unconditionally (unlike new_msg above which gates on currentChannelId).
    const unsubTrain = manager.subscribe('train_updated', (data: unknown) => {
      const payload = data as { train_id?: string } | null
      if (!payload || typeof payload.train_id !== 'string') return
      queryClient.invalidateQueries({ queryKey: ['train', payload.train_id] })
    })

    const unsubVote = manager.subscribe('vote_updated', (data: unknown) => {
      const payload = data as { vote_id?: string } | null
      if (!payload || typeof payload.vote_id !== 'string') return
      queryClient.invalidateQueries({ queryKey: ['vote', payload.vote_id] })
    })

    const unsubReconnect = manager.onReconnect(() => {
      if (currentChannelId) {
        manager.subscribeChannel(currentChannelId)
        queryClient.invalidateQueries({ queryKey: ['messages', currentChannelId] })
      }
    })

    return () => {
      manager.unsubscribeChannel(currentChannelId)
      unsubMsg()
      unsubMsgUpdated()
      unsubTrain()
      unsubVote()
      unsubReconnect()
    }
  }, [currentChannelId, manager, queryClient])
}
