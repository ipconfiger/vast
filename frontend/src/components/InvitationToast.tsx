import { useEffect, useState, useCallback } from 'react'
import { X, Check, Bell, Hash } from 'lucide-react'
import { usePermissionStore } from '../stores/permissionStore'
import { useRespondToInvitation } from '../api/permissions'
import type { InvitationWithChannel } from '../types'

export function InvitationToast() {
  const [visible, setVisible] = useState<InvitationWithChannel | null>(null)
  const [animating, setAnimating] = useState(false)
  const pendingInvitations = usePermissionStore((s) => s.pendingInvitations)
  const removeInvitation = usePermissionStore((s) => s.removeInvitation)
  const respondToInvitation = useRespondToInvitation()

  const dismiss = useCallback(() => {
    setAnimating(false)
    setTimeout(() => setVisible(null), 200)
  }, [])

  useEffect(() => {
    if (pendingInvitations.length > 0 && !visible) {
      const next = pendingInvitations[0]
      setVisible(next)
      requestAnimationFrame(() => setAnimating(true))
    }
  }, [pendingInvitations, visible])

  const handleRespond = (invitationId: string, action: 'accept' | 'decline') => {
    respondToInvitation.mutate(
      { invitationId, action },
      {
        onSuccess: () => {
          removeInvitation(invitationId)
          dismiss()
        },
      },
    )
  }

  if (!visible) return null

  return (
    <div
      className={`fixed bottom-6 right-6 z-50 max-w-sm transition-all duration-200 ${
        animating
          ? 'translate-y-0 opacity-100'
          : 'translate-y-4 opacity-0'
      }`}
    >
      <div className="rounded-xl border border-zinc-800 bg-zinc-900/95 p-4 shadow-2xl shadow-black/50 backdrop-blur">
        <div className="flex items-start gap-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-indigo-500/10 border border-indigo-500/20">
            <Bell className="h-4 w-4 text-indigo-400" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium text-zinc-100">
              Channel Invitation
            </p>
            <p className="mt-0.5 flex items-center gap-1 text-xs text-zinc-400">
              <Hash className="h-3 w-3" />
              {visible.channel?.name ?? visible.channel_id.slice(0, 8)}
            </p>
            {visible.inviter && (
              <p className="mt-0.5 text-xs text-zinc-500">
                Invited by {visible.inviter.display_name}
              </p>
            )}
          </div>
          <button
            onClick={dismiss}
            className="rounded-md p-1 text-zinc-600 hover:text-zinc-400 transition-colors"
            aria-label="Dismiss"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        <div className="mt-3 flex items-center gap-2">
          <button
            onClick={() => handleRespond(visible.id, 'accept')}
            disabled={respondToInvitation.isPending}
            className="flex-1 rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-indigo-500 disabled:opacity-50"
          >
            <Check className="mr-1 inline h-3 w-3" />
            Accept
          </button>
          <button
            onClick={() => handleRespond(visible.id, 'decline')}
            disabled={respondToInvitation.isPending}
            className="flex-1 rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-xs text-zinc-300 transition-colors hover:border-zinc-600 hover:bg-zinc-700 disabled:opacity-50"
          >
            <X className="mr-1 inline h-3 w-3" />
            Decline
          </button>
        </div>
      </div>
    </div>
  )
}
