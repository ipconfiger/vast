import { usePresenceStore } from '../stores/presenceStore'
import { useUserStore } from '../stores/userStore'
import { useAuthStore } from '../stores/authStore'

interface TypingIndicatorProps {
  channelId: string
}

export function TypingIndicator({ channelId }: TypingIndicatorProps) {
  const typingUsers = usePresenceStore((s) => s.typingUsers.get(channelId))
  const user = useAuthStore((s) => s.user)
  const getName = useUserStore((s) => s.getName)

  if (!typingUsers || typingUsers.size === 0) return null

  const names: string[] = []
  for (const userId of typingUsers) {
    if (userId === user?.id) continue
    names.push(getName(userId) ?? userId.slice(0, 8))
  }

  if (names.length === 0) return null

  const label =
    names.length === 1
      ? `${names[0]} is typing`
      : names.length === 2
        ? `${names[0]} and ${names[1]} are typing`
        : `${names.slice(0, 2).join(', ')} and ${names.length - 2} others are typing`

  return (
    <div className="typing-indicator flex items-center gap-2 border-t border-zinc-800 bg-zinc-900/60 px-4 py-1.5 text-xs text-zinc-400">
      <span>{label}</span>
      <span className="inline-flex gap-0.5">
        <span className="animate-typing-dot inline-block h-1 w-1 rounded-full bg-zinc-500" style={{ animationDelay: '0ms' }} />
        <span className="animate-typing-dot inline-block h-1 w-1 rounded-full bg-zinc-500" style={{ animationDelay: '200ms' }} />
        <span className="animate-typing-dot inline-block h-1 w-1 rounded-full bg-zinc-500" style={{ animationDelay: '400ms' }} />
      </span>
    </div>
  )
}
