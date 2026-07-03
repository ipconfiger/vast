import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { getWsManager } from './useWebSocket'
import { useChannelStore } from '../stores/channelStore'

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
      console.debug('[cursorSync] new_msg for channel:', msgChannelId?.slice(0, 8), 'current:', currentChannelId?.slice(0, 8))
      if (msgChannelId === currentChannelId) {
        queryClient.invalidateQueries({ queryKey: ['messages', currentChannelId] })
      }
    })

    // Train queries are keyed by train_id, not channel — invalidate
    // unconditionally (unlike new_msg above which gates on currentChannelId).
    const unsubTrain = manager.subscribe('train_updated', (data: unknown) => {
      const payload = data as { train_id?: string } | null
      if (!payload || typeof payload.train_id !== 'string') return
      queryClient.invalidateQueries({ queryKey: ['train', payload.train_id] })
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
      unsubTrain()
      unsubReconnect()
    }
  }, [currentChannelId, manager, queryClient])
}
