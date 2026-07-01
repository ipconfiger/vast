import { useState } from 'react'
import { UserPlus, Loader2, Check } from 'lucide-react'
import { useSendJoinRequest } from '../api/permissions'

interface JoinRequestButtonProps {
  channelId: string
  isMember: boolean
}

export function JoinRequestButton({ channelId, isMember }: JoinRequestButtonProps) {
  const [requested, setRequested] = useState(false)
  const sendRequest = useSendJoinRequest()

  if (isMember) return null

  if (requested || sendRequest.isSuccess) {
    return (
      <div className="flex items-center gap-1.5 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-1.5 text-xs text-emerald-400">
        <Check className="h-3.5 w-3.5" />
        Requested
      </div>
    )
  }

  return (
    <button
      onClick={() => {
        sendRequest.mutate(channelId, {
          onSuccess: () => setRequested(true),
        })
      }}
      disabled={sendRequest.isPending}
      className="flex items-center gap-1.5 rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs text-zinc-300 transition-colors hover:border-zinc-600 hover:bg-zinc-700 disabled:opacity-50 disabled:cursor-not-allowed"
    >
      {sendRequest.isPending ? (
        <>
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Sending...
        </>
      ) : (
        <>
          <UserPlus className="h-3.5 w-3.5" />
          Request to Join
        </>
      )}
    </button>
  )
}
