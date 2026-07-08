import { useEffect } from 'react'
import { useParams, useNavigate } from 'react-router'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Loader2 } from 'lucide-react'
import { apiClient, ApiClientError } from '../api/client'
import { useAuthStore } from '../stores/authStore'

interface DmChannel {
  id: string
  name: string
  is_direct: boolean
  is_group_dm: boolean
  created_at: number
}

export function DirectMessagePage() {
  const { userId } = useParams<{ userId: string }>()
  const navigate = useNavigate()
  const me = useAuthStore((s) => s.user)
  const queryClient = useQueryClient()

  const {
    data: dmChannel,
    error,
  } = useQuery<DmChannel>({
    queryKey: ['dm', userId],
    queryFn: async () => {
      const data = await apiClient<DmChannel>('/dm', {
        method: 'POST',
        body: JSON.stringify({ user_ids: [me!.id, userId!] }),
      })
      queryClient.invalidateQueries({ queryKey: ['dms'] })
      return data
    },
    enabled: !!userId && !!me,
    staleTime: 0,
  })

  useEffect(() => {
    if (dmChannel) {
      navigate(`/channels/${dmChannel.id}`, { replace: true })
    }
  }, [dmChannel, navigate])

  if (!userId || !me) {
    return (
      <div className="flex h-screen items-center justify-center bg-zinc-950 text-zinc-500">
        <button
          onClick={() => navigate('/channels')}
          className="flex items-center gap-2 text-sm hover:text-zinc-300"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to channels
        </button>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex h-screen flex-col items-center justify-center bg-zinc-950 text-zinc-100">
        <p className="text-sm text-red-400">
          {error instanceof ApiClientError ? error.message : 'Could not open DM.'}
        </p>
        <button
          onClick={() => navigate('/channels')}
          className="mt-4 flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-300"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to channels
        </button>
      </div>
    )
  }

  // isLoading or waiting for dmChannel before redirect triggers
  return (
    <div className="flex h-screen items-center justify-center bg-zinc-950">
      <Loader2 className="h-6 w-6 animate-spin text-zinc-400" />
    </div>
  )
}
