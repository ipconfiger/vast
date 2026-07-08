import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from './client'
import type {
  JoinRequestWithUser,
  ChannelMemberWithUser,
} from '../types'

export function useJoinRequests(channelId: string | null) {
  return useQuery({
    queryKey: ['join-requests', channelId],
    queryFn: () =>
      apiClient<JoinRequestWithUser[]>(`/channels/${channelId}/join-requests`),
    enabled: !!channelId,
  })
}

export function useSendJoinRequest() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (channelId: string) =>
      apiClient<void>(`/channels/${channelId}/join-request`, {
        method: 'POST',
      }),
    onSuccess: (_data, channelId) => {
      queryClient.invalidateQueries({ queryKey: ['join-requests', channelId] })
    },
  })
}

export function useApproveJoinRequest() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      channelId,
      requestId,
    }: {
      channelId: string
      requestId: string
    }) =>
      apiClient<void>(`/channels/${channelId}/join-requests/${requestId}/approve`, {
        method: 'POST',
      }),
    onSuccess: (_data, { channelId }) => {
      queryClient.invalidateQueries({ queryKey: ['join-requests', channelId] })
      queryClient.invalidateQueries({ queryKey: ['channel-members', channelId] })
    },
  })
}

export function useRejectJoinRequest() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      channelId,
      requestId,
    }: {
      channelId: string
      requestId: string
    }) =>
      apiClient<void>(`/channels/${channelId}/join-requests/${requestId}/reject`, {
        method: 'POST',
      }),
    onSuccess: (_data, { channelId }) => {
      queryClient.invalidateQueries({ queryKey: ['join-requests', channelId] })
    },
  })
}

export function usePendingRequestsCount() {
  return useQuery({
    queryKey: ['pending-requests-count'],
    queryFn: async () => {
      const result = await apiClient<{ count: number }>(
        '/channels/join-requests/pending-count',
      )
      return result.count
    },
    refetchInterval: 30_000,
  })
}

export function useRespondToInvitation() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      invitationId,
      action,
    }: {
      invitationId: string
      action: 'accept' | 'decline'
    }) =>
      apiClient<void>(`/invitations/${invitationId}/${action}`, {
        method: 'POST',
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['channels'] })
    },
  })
}

export function useChannelMembers(channelId: string | null) {
  return useQuery({
    queryKey: ['channel-members', channelId],
    queryFn: () =>
      apiClient<ChannelMemberWithUser[]>(`/channels/${channelId}/members`),
    enabled: !!channelId,
  })
}

export function useRemoveMember() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      channelId,
      userId,
    }: {
      channelId: string
      userId: string
    }) =>
      apiClient<void>(`/channels/${channelId}/members/${userId}`, {
        method: 'DELETE',
      }),
    onSuccess: (_data, { channelId }) => {
      queryClient.invalidateQueries({ queryKey: ['channel-members', channelId] })
    },
  })
}

export function useUpdateChannel() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      channelId,
      data,
    }: {
      channelId: string
      data: { name?: string; description?: string }
    }) =>
      apiClient<void>(`/channels/${channelId}`, {
        method: 'PATCH',
        body: JSON.stringify(data),
      }),
    onSuccess: (_data, { channelId }) => {
      queryClient.invalidateQueries({ queryKey: ['channel', channelId] })
      queryClient.invalidateQueries({ queryKey: ['channels'] })
    },
  })
}

export function useArchiveChannel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (channelId: string) =>
      apiClient<void>(`/channels/${channelId}/archive`, { method: 'POST' }),
    onSuccess: (_data, channelId) => {
      queryClient.invalidateQueries({ queryKey: ['channel', channelId] })
      queryClient.invalidateQueries({ queryKey: ['channels'] })
    },
  })
}

export function useUnarchiveChannel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (channelId: string) =>
      apiClient<void>(`/channels/${channelId}/unarchive`, { method: 'POST' }),
    onSuccess: (_data, channelId) => {
      queryClient.invalidateQueries({ queryKey: ['channel', channelId] })
      queryClient.invalidateQueries({ queryKey: ['channels'] })
    },
  })
}

export function useAddBot() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ channelId, botId }: { channelId: string; botId: string }) =>
      apiClient<{ ok: boolean }>(`/channels/${channelId}/bots`, {
        method: 'POST',
        body: JSON.stringify({ bot_id: botId }),
      }),
    onSuccess: (_data, { channelId }) => {
      queryClient.invalidateQueries({ queryKey: ['channel-members', channelId] })
    },
  })
}
