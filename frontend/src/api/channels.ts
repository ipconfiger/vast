import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from './client'
import type { Channel, Message } from '../types'
import { useChannelStore } from '../stores/channelStore'
import { useMessageStore } from '../stores/messageStore'

export function useChannels() {
  const setChannels = useChannelStore((s) => s.setChannels)

  return useQuery({
    queryKey: ['channels'],
    queryFn: async () => {
      const data = await apiClient<{ channels: Channel[] }>('/channels')
      setChannels(data.channels)
      return data.channels
    },
  })
}

export function useChannel(channelId: string | null) {
  return useQuery({
    queryKey: ['channel', channelId],
    queryFn: () => apiClient<Channel>(`/channels/${channelId}`),
    enabled: !!channelId,
  })
}

export function useCreateChannel() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (data: { name: string; description?: string; type?: 'public' | 'private' }) =>
      apiClient<Channel>('/channels', { method: 'POST', body: JSON.stringify(data) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['channels'] })
    },
  })
}

export function useMessages(channelId: string | null) {
  const setMessages = useMessageStore((s) => s.setMessages)

  return useQuery({
    queryKey: ['messages', channelId],
    queryFn: async () => {
      const data = await apiClient<{ messages: Message[]; next_cursor: number; has_more: boolean }>(`/channels/${channelId}/messages`)
      setMessages(channelId!, data.messages, data.next_cursor?.toString() ?? null)
      return data.messages
    },
    enabled: !!channelId,
  })
}

export function useSendMessage(channelId: string) {
  const addMessage = useMessageStore((s) => s.addMessage)
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (data: { msg_type: string; payload: unknown; thread_parent_id?: string | null }) =>
      apiClient<Message>(`/channels/${channelId}/messages`, {
        method: 'POST',
        body: JSON.stringify(data),
      }),
    onSuccess: (message) => {
      addMessage(channelId, message)
      queryClient.invalidateQueries({ queryKey: ['messages', channelId] })
    },
  })
}
