import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient, refreshAccessToken } from './client'
import type { Channel, Message } from '../types'
import { useMessageStore } from '../stores/messageStore'
import { useAuthStore } from '../stores/authStore'

const API_BASE = import.meta.env.VITE_API_BASE || '/api'

export interface PublicBot {
  id: string
  name: string
  display_name: string
}

export async function listPublicBots(): Promise<PublicBot[]> {
  return apiClient<PublicBot[]>('/bots')
}

export function usePublicBots() {
  return useQuery({
    queryKey: ['bots'],
    queryFn: listPublicBots,
  })
}

export function useChannels() {
  return useQuery({
    queryKey: ['channels'],
    queryFn: async () => {
      const data = await apiClient<{ channels: Channel[] }>('/channels')
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
      queryClient.refetchQueries({ queryKey: ['discover-channels'] })
    },
  })
}

export function useMessages(channelId: string | null) {
  return useQuery({
    queryKey: ['messages', channelId],
    queryFn: async () => {
      const data = await apiClient<{ messages: Message[]; next_cursor: number; has_more: boolean }>(`/channels/${channelId}/messages`)
      return data.messages
    },
    enabled: !!channelId,
  })
}

interface DiscoverChannel {
  id: string
  name: string
  description: string
  owner_name: string
  member_count: number
  is_member: boolean
}

export function useDiscoverChannels() {
  return useQuery({
    queryKey: ['discover-channels'],
    queryFn: () =>
      apiClient<{ channels: DiscoverChannel[] }>('/channels/discover'),
    staleTime: 0,
    refetchOnMount: true,
  })
}

export function useJoinChannel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (channelId: string) =>
      apiClient('/channels/' + channelId + '/join-request', { method: 'POST' }),
    onSuccess: () => {
      queryClient.refetchQueries({ queryKey: ['discover-channels'] })
    },
  })
}

export async function downloadChannelArchive(channelId: string, channelName: string): Promise<void> {
  const store = useAuthStore.getState()
  let token = store.token
  if (!token) {
    console.error('Cannot download archive: not authenticated')
    return
  }
  if (store.isTokenExpired()) {
    const newToken = await refreshAccessToken()
    if (!newToken) throw new Error('Session expired')
    token = newToken
  }

  let res = await fetch(`${API_BASE}/channels/${channelId}/archive/download`, {
    headers: { Authorization: `Bearer ${token}` },
  })

  if (res.status === 401) {
    const newToken = await refreshAccessToken()
    if (!newToken) throw new Error('Session expired')
    res = await fetch(`${API_BASE}/channels/${channelId}/archive/download`, {
      headers: { Authorization: `Bearer ${newToken}` },
    })
  }

  if (!res.ok) throw new Error(`Download failed: ${res.status}`)

  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  const safeName = channelName.replace(/[\/\\:*?"<>|\x00-\x1f]/g, '_').replace(/^[.\s]+/, '').replace(/[.\s]+$/, '') || 'channel'
  a.download = `${safeName}-archive.zip`
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
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
