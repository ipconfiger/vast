import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from './client'

export interface DmChannel {
  id: string
  name: string
  description: string
  owner_id: string | null
  is_direct: boolean
  is_group_dm: boolean
  is_archived: boolean
  created_at: number
}

export function useDms() {
  return useQuery({
    queryKey: ['dms'],
    queryFn: async () => {
      const data = await apiClient<{ channels: DmChannel[] }>('/dm')
      return data.channels
    },
  })
}

export function useCreateDm() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (userIds: string[]) =>
      apiClient<DmChannel>('/dm', {
        method: 'POST',
        body: JSON.stringify({ user_ids: userIds }),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dms'] })
    },
  })
}
