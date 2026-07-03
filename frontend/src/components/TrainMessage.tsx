import { useState, type KeyboardEvent } from 'react'
import { Loader2 } from 'lucide-react'
import { useTrain, useJoinTrain } from '../api/trains'
import { useAuthStore } from '../stores/authStore'
import { TrainRepliesModal } from './TrainRepliesModal'
import type { TrainReply } from '../types'

const AVATAR_COLORS = [
  'bg-rose-500/80 text-rose-50',
  'bg-orange-500/80 text-orange-50',
  'bg-amber-500/80 text-amber-50',
  'bg-emerald-500/80 text-emerald-50',
  'bg-teal-500/80 text-teal-50',
  'bg-sky-500/80 text-sky-50',
  'bg-indigo-500/80 text-indigo-50',
  'bg-fuchsia-500/80 text-fuchsia-50',
]

function colorFor(userId: string): string {
  let hash = 0
  for (let i = 0; i < userId.length; i++) hash = (hash * 31 + userId.charCodeAt(i)) | 0
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length]
}

function ReplyRow({ reply }: { reply: TrainReply }) {
  const name = reply.display_name?.trim() || reply.username
  return (
    <div className="flex items-center gap-2 py-1">
      <div
        className={`flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-full text-xs font-semibold ${colorFor(reply.user_id)}`}
      >
        {name.charAt(0).toUpperCase()}
      </div>
      <span className="flex-shrink-0 text-xs font-semibold text-zinc-200">{reply.username}</span>
      <span className="min-w-0 truncate text-xs text-zinc-300">{reply.content}</span>
    </div>
  )
}

interface TrainMessageProps {
  trainId: string
  title: string
  channelId: string
}

export function TrainMessage({ trainId, title }: TrainMessageProps) {
  const { data: train, isLoading, isError } = useTrain(trainId)
  const joinTrain = useJoinTrain()
  const currentUserId = useAuthStore((s) => s.user?.id)
  const [isJoining, setIsJoining] = useState(false)
  const [draft, setDraft] = useState('')
  const [showAll, setShowAll] = useState(false)

  const replies = train?.replies ?? []
  const hasJoined = !!currentUserId && replies.some((r) => r.user_id === currentUserId)
  const inlineReplies = replies.slice(-3)

  const handleSubmit = async () => {
    const content = draft.trim()
    if (!content || joinTrain.isPending) return
    try {
      await joinTrain.mutateAsync({ trainId, content })
      setDraft('')
      setIsJoining(false)
    } catch {
      // ApiClientError surfaces a 409 on duplicate joins; the WS-driven
      // refetch will flip the button to "已接龙" anyway. Swallow here.
    }
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      void handleSubmit()
    }
    if (e.key === 'Escape') {
      setDraft('')
      setIsJoining(false)
    }
  }

  return (
    <div className="train-message w-full max-w-md rounded-lg border border-zinc-700 bg-zinc-800/50">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-zinc-700/60 px-3 py-2">
        <span className="text-sm">📋</span>
        <span className="truncate text-sm font-bold text-zinc-100">{title}</span>
        <span className="ml-auto flex-shrink-0 text-xs text-zinc-500">
          {replies.length} 人参与
        </span>
      </div>

      {/* Body */}
      <div className="px-3 py-1">
        {isLoading ? (
          <div className="flex items-center justify-center py-3">
            <Loader2 className="h-4 w-4 animate-spin text-zinc-500" />
          </div>
        ) : isError ? (
          <p className="py-3 text-center text-xs text-red-400">接龙加载失败</p>
        ) : replies.length === 0 ? (
          <p className="py-2 text-center text-xs text-zinc-500">还没有人接龙，快来第一个加入吧</p>
        ) : (
          inlineReplies.map((reply) => <ReplyRow key={reply.user_id} reply={reply} />)
        )}
      </div>

      {/* Inline join form */}
      {isJoining && !hasJoined && (
        <div className="border-t border-zinc-700/60 px-3 py-2">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="输入接龙内容…"
            rows={2}
            autoFocus
            className="w-full resize-none rounded-md border border-zinc-600 bg-zinc-900 px-2 py-1 text-sm text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/50 focus:outline-none"
          />
          <div className="mt-1 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => { setDraft(''); setIsJoining(false) }}
              className="rounded-md px-2 py-1 text-xs text-zinc-400 hover:text-zinc-200"
            >
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={!draft.trim() || joinTrain.isPending}
              className="rounded-md bg-indigo-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {joinTrain.isPending ? '提交中…' : '提交'}
            </button>
          </div>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center gap-2 border-t border-zinc-700/60 px-3 py-2">
        {hasJoined ? (
          <button
            type="button"
            disabled
            className="cursor-default rounded-md bg-zinc-700/50 px-3 py-1 text-xs font-medium text-zinc-400"
          >
            ✓ 已接龙
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setIsJoining((v) => !v)}
            disabled={isLoading}
            className="rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isJoining ? '收起' : '+ 加入接龙'}
          </button>
        )}
        {replies.length > 3 && (
          <button
            type="button"
            onClick={() => setShowAll(true)}
            className="ml-auto text-xs text-indigo-400 hover:text-indigo-300"
          >
            查看全部 ({replies.length})
          </button>
        )}
      </div>

      {showAll && (
        <TrainRepliesModal
          trainId={trainId}
          isOpen={showAll}
          onClose={() => setShowAll(false)}
        />
      )}
    </div>
  )
}
