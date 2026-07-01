import { useParams, useNavigate } from 'react-router'
import {
  ArrowLeft,
  Loader2,
  Check,
  X,
  User,
  Clock,
} from 'lucide-react'
import {
  useJoinRequests,
  useApproveJoinRequest,
  useRejectJoinRequest,
} from '../api/permissions'
import { useAuthStore } from '../stores/authStore'
import dayjs from 'dayjs'

export function RequestsPage() {
  const { channelId } = useParams<{ channelId: string }>()
  const navigate = useNavigate()
  const { data: requests, isLoading } = useJoinRequests(channelId ?? null)
  const approveRequest = useApproveJoinRequest()
  const rejectRequest = useRejectJoinRequest()
  const user = useAuthStore((s) => s.user)

  const pending = requests?.filter((r) => r.status === 'pending') ?? []

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <div className="mx-auto w-full max-w-2xl px-6 py-8">
        <button
          onClick={() => navigate(`/channels/${channelId}`)}
          className="mb-6 flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to channel
        </button>

        <div className="mb-6">
          <h1 className="text-xl font-semibold text-zinc-100">Join Requests</h1>
          <p className="mt-1 text-sm text-zinc-500">
            Manage pending requests to join this channel
          </p>
        </div>

        {isLoading ? (
          <div className="flex items-center justify-center py-16">
            <Loader2 className="h-6 w-6 animate-spin text-zinc-600" />
          </div>
        ) : pending.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <Clock className="mb-3 h-10 w-10 text-zinc-700" />
            <p className="text-sm text-zinc-500">No pending join requests</p>
            <p className="mt-1 text-xs text-zinc-600">
              Requests will appear here when users request to join
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            {pending.map((request) => (
              <div
                key={request.id}
                className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-900/60 px-4 py-3"
              >
                <div className="flex items-center gap-3">
                  <div className="flex h-8 w-8 items-center justify-center rounded-full bg-zinc-800">
                    <User className="h-4 w-4 text-zinc-400" />
                  </div>
                  <div>
                    <p className="text-sm font-medium text-zinc-200">
                      {request.user?.display_name ?? request.user?.username ?? request.user_id.slice(0, 8)}
                    </p>
                    <p className="text-xs text-zinc-500">
                      Requested {dayjs(request.created_at).format('MMM D, h:mm A')}
                    </p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() =>
                      approveRequest.mutate({
                        channelId: channelId!,
                        requestId: request.id,
                      })
                    }
                    disabled={approveRequest.isPending}
                    className="flex items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-1.5 text-xs text-emerald-400 transition-colors hover:bg-emerald-500/20 disabled:opacity-50"
                  >
                    <Check className="h-3.5 w-3.5" />
                    Approve
                  </button>
                  <button
                    onClick={() =>
                      rejectRequest.mutate({
                        channelId: channelId!,
                        requestId: request.id,
                      })
                    }
                    disabled={rejectRequest.isPending}
                    className="flex items-center gap-1 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-1.5 text-xs text-red-400 transition-colors hover:bg-red-500/20 disabled:opacity-50"
                  >
                    <X className="h-3.5 w-3.5" />
                    Reject
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {user && (
          <div className="mt-6 border-t border-zinc-800 pt-4">
            <p className="text-xs text-zinc-600">
              Logged in as{' '}
              <span className="text-zinc-400">{user.display_name}</span>
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
