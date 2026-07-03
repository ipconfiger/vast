import { Loader2 } from 'lucide-react'
import { useVote, useCastVote } from '../api/votes'

interface VoteMessageProps {
  voteId: string
  title: string
  channelId: string
}

export function VoteMessage({ voteId, title }: VoteMessageProps) {
  const { data: vote, isLoading, isError } = useVote(voteId)
  const castVote = useCastVote()

  const handleVote = (optionId: string) => {
    if (vote?.myVote !== null && vote?.myVote !== undefined) return
    castVote.mutate({ voteId, optionId })
  }

  if (isLoading) {
    return (
      <div className="vote-message w-full max-w-md rounded-lg border border-zinc-700 bg-zinc-800/50">
        <div className="flex items-center justify-center py-6">
          <Loader2 className="h-4 w-4 animate-spin text-zinc-500" />
        </div>
      </div>
    )
  }

  if (isError || !vote) {
    return (
      <div className="vote-message w-full max-w-md rounded-lg border border-zinc-700 bg-zinc-800/50">
        <p className="py-4 text-center text-xs text-red-400">投票加载失败</p>
      </div>
    )
  }

  const totalVotes = vote.options.reduce((sum, opt) => sum + opt.count, 0)
  const hasVoted = vote.myVote !== null

  return (
    <div className="vote-message w-full max-w-md rounded-lg border border-zinc-700 bg-zinc-800/50">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-zinc-700/60 px-3 py-2">
        <span className="text-sm">📊</span>
        <span className="truncate text-sm font-bold text-zinc-100">{title}</span>
      </div>

      {/* Options */}
      <div className="flex flex-col gap-2 px-3 py-2">
        {vote.options.map((opt) => {
          const percentage = totalVotes > 0 ? Math.round((opt.count / totalVotes) * 100) : 0
          const isMyChoice = vote.myVote === opt.id

          return (
            <div key={opt.id} className="flex items-center gap-2">
              <span className="w-20 flex-shrink-0 truncate text-xs text-zinc-300">
                {opt.text}
              </span>
              <div className="h-5 flex-1 overflow-hidden rounded bg-zinc-700">
                <div
                  className={`h-full transition-all duration-300 ${
                    isMyChoice ? 'bg-emerald-500' : 'bg-indigo-500'
                  }`}
                  style={{ width: `${percentage}%` }}
                />
              </div>
              <span className="w-16 flex-shrink-0 text-xs text-zinc-500">
                {opt.count} ({percentage}%)
              </span>
              <div className="flex-shrink-0">
                {isMyChoice ? (
                  <span className="text-xs font-medium text-emerald-400">✓ 已投</span>
                ) : hasVoted ? (
                  <button
                    type="button"
                    disabled
                    className="cursor-not-allowed rounded-md bg-zinc-700/50 px-2 py-0.5 text-xs text-zinc-500 opacity-50"
                  >
                    投票
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => handleVote(opt.id)}
                    disabled={castVote.isPending}
                    className="rounded-md bg-indigo-600 px-2 py-0.5 text-xs font-medium text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    投票
                  </button>
                )}
              </div>
            </div>
          )
        })}
      </div>

      {/* Footer */}
      <div className="border-t border-zinc-700/60 px-3 py-2">
        <span className="text-xs text-zinc-500">共 {totalVotes} 人投票</span>
      </div>
    </div>
  )
}
