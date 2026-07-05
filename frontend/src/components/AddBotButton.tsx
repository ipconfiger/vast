import { useState } from 'react'
import { Bot, Plus, X, Loader2, ChevronDown } from 'lucide-react'
import { useAddBot } from '../api/permissions'
import { usePublicBots } from '../api/channels'
import { ApiClientError } from '../api/client'

interface AddBotButtonProps {
  channelId: string
}

export function AddBotButton({ channelId }: AddBotButtonProps) {
  const [open, setOpen] = useState(false)
  const [selectedBotId, setSelectedBotId] = useState('')
  const addBot = useAddBot()
  const botsQuery = usePublicBots()

  const submitting = addBot.isPending
  const errorMessage = extractErrorMessage(addBot.error)

  function reset() {
    setSelectedBotId('')
    setOpen(false)
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!selectedBotId) return
    addBot.mutate(
      { channelId, botId: selectedBotId },
      {
        onSuccess: () => reset(),
      },
    )
  }

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="flex w-full items-center gap-2 rounded-lg border border-dashed border-zinc-700 px-3 py-2 text-sm text-zinc-400 transition-colors hover:border-indigo-500/50 hover:bg-indigo-500/5 hover:text-indigo-400"
      >
        <Bot className="h-4 w-4" />
        Add Bot
      </button>
    )
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="space-y-2 rounded-lg border border-zinc-700 bg-zinc-800/50 p-3"
    >
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-xs font-medium text-zinc-300">
          <Plus className="h-3.5 w-3.5" />
          Add Bot to Channel
        </span>
        <button
          type="button"
          onClick={reset}
          disabled={submitting}
          className="rounded p-0.5 text-zinc-500 hover:text-zinc-300 disabled:opacity-50"
          aria-label="Cancel add bot"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      {botsQuery.isLoading ? (
        <div className="flex items-center gap-2 py-1.5 text-xs text-zinc-500">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Loading bots...
        </div>
      ) : botsQuery.isError ? (
        <p className="py-1.5 text-xs text-red-400">Failed to load bots.</p>
      ) : !botsQuery.data || botsQuery.data.length === 0 ? (
        <p className="py-1.5 text-xs text-zinc-500">No available bots to add.</p>
      ) : (
        <div className="relative">
          <select
            value={selectedBotId}
            onChange={(e) => setSelectedBotId(e.target.value)}
            disabled={submitting}
            className="w-full appearance-none rounded-md border border-zinc-700 bg-zinc-900 px-3 py-1.5 pr-8 text-sm text-zinc-100 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500/50 disabled:opacity-60"
            aria-label="Select a bot"
          >
            <option value="">Select a bot...</option>
            {botsQuery.data.map((b) => (
              <option key={b.id} value={b.id}>
                @{b.name}
                {b.display_name ? ` · ${b.display_name}` : ''}
              </option>
            ))}
          </select>
          <ChevronDown className="pointer-events-none absolute right-2 top-1/2 h-4 w-4 -translate-y-1/2 text-zinc-500" />
        </div>
      )}

      {errorMessage && (
        <p className="text-xs text-red-400" role="alert">
          {errorMessage}
        </p>
      )}

      <button
        type="submit"
        disabled={submitting || !selectedBotId}
        className="flex w-full items-center justify-center gap-1.5 rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {submitting ? (
          <>
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Adding...
          </>
        ) : (
          <>
            <Plus className="h-3.5 w-3.5" />
            Add Bot
          </>
        )}
      </button>
    </form>
  )
}

function extractErrorMessage(error: unknown): string {
  if (!error) return ''
  if (error instanceof ApiClientError) {
    if (error.status === 403) return 'Only the channel owner can add bots.'
    if (error.status === 409) return 'Bot is already a member of this channel.'
    if (error.status === 400) return 'Bot is not active.'
    if (error.status === 404) return 'Bot not found.'
    return error.message
  }
  if (error instanceof Error) return error.message
  return 'Failed to add bot'
}
