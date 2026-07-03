import { useState, useCallback, useRef, useEffect, type KeyboardEvent } from 'react'
import { SmilePlus } from 'lucide-react'
import { toggleReaction } from '../api/reactions'
import { useReactionStore } from '../stores/reactionStore'
import { useAuthStore } from '../stores/authStore'

const QUICK_EMOJIS = ['👍', '❤️', '😂', '😮', '😢', '👏', '🔥', '🎉', '✅', '🚀']

interface ReactionPickerProps {
  messageId: string
  isOwn: boolean
}

export function ReactionPicker({ messageId, isOwn }: ReactionPickerProps) {
  const [open, setOpen] = useState(false)
  const [pending, setPending] = useState<string | null>(null)
  const pickerRef = useRef<HTMLDivElement>(null)
  const { reactionsByMessage } = useReactionStore()
  const user = useAuthStore((s) => s.user)

  const reactions = reactionsByMessage.get(String(messageId)) ?? []

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    if (open) {
      document.addEventListener('mousedown', handleClickOutside)
    }
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [open])

  const handleToggle = useCallback(
    async (emoji: string) => {
      if (pending) return
      setPending(emoji)
      try {
        const result = await toggleReaction(messageId, emoji)
        console.log('[reaction] toggled:', emoji, result)
      } catch (err) {
        console.error('[reaction] failed:', err)
      } finally {
        setPending(null)
      }
    },
    [messageId, pending],
  )

  const handleKeyDown = (e: KeyboardEvent<HTMLButtonElement>, emoji: string) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      handleToggle(emoji)
    }
    if (e.key === 'Escape') {
      setOpen(false)
    }
  }

  const userHasReacted = (emoji: string): boolean => {
    return reactions.some((r) => r.user_id === user?.id && r.emoji === emoji)
  }

  return (
    <div ref={pickerRef} className="reaction-picker relative inline-flex items-center">
      <button
        onClick={() => setOpen((prev) => !prev)}
        className={`rounded p-1 text-zinc-500 hover:bg-zinc-700 hover:text-zinc-200 transition-all ${
          isOwn ? 'opacity-0 group-hover:opacity-100' : ''
        }`}
        aria-label="Add reaction"
        aria-expanded={open}
      >
        <SmilePlus className="h-4 w-4" />
      </button>

      {open && (
        <div className={`absolute bottom-full mb-2 z-50 flex items-center gap-0.5 rounded-lg border border-zinc-700 bg-zinc-800 px-1.5 py-1.5 shadow-xl whitespace-nowrap ${isOwn ? 'right-0' : 'left-0'}`}>
          {QUICK_EMOJIS.map((emoji) => {
            const active = userHasReacted(emoji)
            return (
              <button
                key={emoji}
                onClick={() => handleToggle(emoji)}
                onKeyDown={(e) => handleKeyDown(e, emoji)}
                disabled={pending === emoji}
                className={`rounded-md p-1.5 text-lg leading-none transition-all hover:scale-110 disabled:opacity-50 ${
                  active
                    ? 'bg-zinc-700 ring-1 ring-blue-500/40'
                    : 'hover:bg-zinc-700'
                }`}
                aria-label={`React with ${emoji}`}
                title={emoji}
              >
                {emoji}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
