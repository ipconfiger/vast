import { useCallback } from 'react'
import { useReactionStore } from '../stores/reactionStore'
import { useAuthStore } from '../stores/authStore'
import { toggleReaction } from '../api/reactions'
import type { Reaction } from '../types'

interface ReactionBarProps {
  messageId: string
}

interface ReactionGroup {
  emoji: string
  count: number
  hasUserReacted: boolean
}

export function ReactionBar({ messageId }: ReactionBarProps) {
  const reactions = useReactionStore((s) => s.reactionsByMessage.get(String(messageId))) ?? []
  const user = useAuthStore((s) => s.user)

  const grouped: ReactionGroup[] = groupReactions(reactions, user?.id)

  const handleToggle = useCallback(
    async (emoji: string) => {
      try {
        await toggleReaction(messageId, emoji)
      } catch {
        // WS will sync
      }
    },
    [messageId],
  )

  if (grouped.length === 0) return null

  return (
    <div className="reaction-bar mt-1 flex flex-wrap items-center gap-1">
      {grouped.map((group) => (
        <button
          key={group.emoji}
          onClick={() => handleToggle(group.emoji)}
          className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs leading-relaxed transition-all hover:border-zinc-500 ${
            group.hasUserReacted
              ? 'border-blue-500/30 bg-blue-500/10 text-blue-300'
              : 'border-zinc-700 bg-zinc-800/50 text-zinc-400 hover:text-zinc-200'
          }`}
          aria-label={`${group.emoji} reaction: ${group.count}`}
        >
          <span className="text-sm leading-none">{group.emoji}</span>
          <span className="tabular-nums">{group.count}</span>
        </button>
      ))}
    </div>
  )
}

function groupReactions(
  reactions: Reaction[],
  userId: string | undefined,
): ReactionGroup[] {
  const map = new Map<string, { count: number; users: Set<string> }>()

  for (const r of reactions) {
    const entry = map.get(r.emoji)
    if (entry) {
      entry.count++
      entry.users.add(r.user_id)
    } else {
      map.set(r.emoji, { count: 1, users: new Set([r.user_id]) })
    }
  }

  return Array.from(map.entries()).map(([emoji, { count, users }]) => ({
    emoji,
    count,
    hasUserReacted: userId ? users.has(userId) : false,
  }))
}
