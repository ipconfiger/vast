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

    const unsubReconnect = manager.onReconnect(() => {
      if (currentChannelId) {
        manager.subscribeChannel(currentChannelId)
        queryClient.invalidateQueries({ queryKey: ['messages', currentChannelId] })
      }
    })

    return () => {
      manager.unsubscribeChannel(currentChannelId)
      unsubMsg()
      unsubReconnect()
    }
  }, [currentChannelId, manager, queryClient])
}
