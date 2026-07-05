import { Crown, Shield, UserMinus, Loader2 } from 'lucide-react'
import { useChannelMembers, useRemoveMember } from '../api/permissions'
import { useAuthStore } from '../stores/authStore'
import { UserAvatar } from './UserAvatar'
import { AddBotButton } from './AddBotButton'
import { getUserDisplayName } from '../utils/user'
import type { ChannelMemberWithUser } from '../types'

interface MemberListProps {
  channelId: string
}

function RoleBadge({ role }: { role: ChannelMemberWithUser['role'] }) {
  switch (role) {
    case 'owner':
      return (
        <span className="inline-flex items-center gap-1 rounded-md bg-amber-500/10 border border-amber-500/20 px-1.5 py-0.5 text-[10px] font-medium text-amber-400">
          <Crown className="h-2.5 w-2.5" />
          Owner
        </span>
      )
    case 'admin':
      return (
        <span className="inline-flex items-center gap-1 rounded-md bg-indigo-500/10 border border-indigo-500/20 px-1.5 py-0.5 text-[10px] font-medium text-indigo-400">
          <Shield className="h-2.5 w-2.5" />
          Admin
        </span>
      )
    case 'member':
    default:
      return null
  }
}

function MemberItem({
  member,
  isCurrentUser,
  isChannelOwner,
  onRemove,
  isRemoving,
}: {
  member: ChannelMemberWithUser
  isCurrentUser: boolean
  isChannelOwner: boolean
  onRemove: () => void
  isRemoving: boolean
}) {
  const displayName = getUserDisplayName(
    member.user?.display_name,
    member.user?.username,
    member.user_id,
  )
  const canRemove =
    isChannelOwner && !isCurrentUser && member.role !== 'owner'

  return (
    <div className="flex items-center justify-between rounded-lg px-3 py-2 transition-colors hover:bg-zinc-800/50">
      <div className="flex items-center gap-3">
        <UserAvatar
          avatarUrl={member.user?.avatar_url}
          displayName={displayName}
          size="sm"
        />
        <div className="flex items-center gap-2">
          <span className="text-sm text-zinc-200">
            {displayName}
            {isCurrentUser && (
              <span className="ml-1 text-xs text-zinc-500">(you)</span>
            )}
          </span>
          <RoleBadge role={member.role} />
        </div>
      </div>
      {canRemove && (
        <button
          onClick={onRemove}
          disabled={isRemoving}
          className="rounded-md p-1 text-zinc-600 hover:text-red-400 hover:bg-red-500/10 transition-colors disabled:opacity-50"
          aria-label={`Remove ${displayName}`}
        >
          {isRemoving ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <UserMinus className="h-4 w-4" />
          )}
        </button>
      )}
    </div>
  )
}

export function MemberList({ channelId }: MemberListProps) {
  const { data: members, isLoading } = useChannelMembers(channelId)
  const removeMember = useRemoveMember()
  const user = useAuthStore((s) => s.user)

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-5 w-5 animate-spin text-zinc-600" />
      </div>
    )
  }

  if (!members || members.length === 0) {
    return (
      <p className="py-4 text-center text-xs text-zinc-600">
        No members found
      </p>
    )
  }

  const channelOwner = members.find((m) => m.role === 'owner')
  const isChannelOwner = channelOwner?.user_id === user?.id

  const memberUserIds = new Set(members.map((m) => m.user_id))

  return (
    <div className="flex flex-col gap-0.5">
      {isChannelOwner && (
        <div className="mb-1">
          <AddBotButton
            channelId={channelId}
            memberUserIds={memberUserIds}
          />
        </div>
      )}
      {members.map((member) => (
        <MemberItem
          key={member.id}
          member={member}
          isCurrentUser={member.user_id === user?.id}
          isChannelOwner={isChannelOwner ?? false}
          onRemove={() =>
            removeMember.mutate({ channelId, userId: member.user_id })
          }
          isRemoving={
            removeMember.isPending &&
            removeMember.variables?.userId === member.user_id
          }
        />
      ))}
    </div>
  )
}
