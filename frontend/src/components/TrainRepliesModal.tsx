import { X, Loader2 } from 'lucide-react'
import { useTrain } from '../api/trains'
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

function formatTimestamp(unix: number): string {
  try {
    return new Date(unix * 1000).toLocaleString()
  } catch {
    return ''
  }
}

function ReplyRow({ reply }: { reply: TrainReply }) {
  const name = reply.display_name?.trim() || reply.username
  return (
    <div className="flex items-start gap-3 py-2.5">
      <div
        className={`flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-sm font-semibold ${colorFor(reply.user_id)}`}
      >
        {name.charAt(0).toUpperCase()}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="text-sm font-semibold text-zinc-200">{reply.username}</span>
          {reply.display_name && (
            <span className="truncate text-xs text-zinc-500">{reply.display_name}</span>
          )}
        </div>
        <p className="mt-0.5 text-sm text-zinc-300 break-words">{reply.content}</p>
      </div>
      <span className="flex-shrink-0 text-xs text-zinc-600">
        {formatTimestamp(reply.created_at)}
      </span>
    </div>
  )
}

interface TrainRepliesModalProps {
  trainId: string
  isOpen: boolean
  onClose: () => void
}

function ModalContent({ trainId, onClose }: { trainId: string; onClose: () => void }) {
  const { data: train, isLoading, isError } = useTrain(trainId)
  const replies = train?.replies ?? []

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        className="relative flex max-h-[80vh] w-full max-w-lg flex-col rounded-2xl border border-zinc-800 bg-zinc-950 shadow-2xl shadow-black/50"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-zinc-100">
              {train?.title ?? '接龙'}
            </h2>
            <p className="mt-0.5 text-xs text-zinc-500">
              {replies.length} 人参与
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-zinc-500 transition-colors hover:text-zinc-300"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-2">
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="h-6 w-6 animate-spin text-zinc-500" />
            </div>
          ) : isError ? (
            <div className="py-8 text-center text-sm text-red-400">接龙加载失败</div>
          ) : replies.length === 0 ? (
            <div className="py-8 text-center text-sm text-zinc-500">还没有人接龙</div>
          ) : (
            <div className="flex flex-col divide-y divide-zinc-800/60">
              {replies.map((reply) => (
                <ReplyRow key={reply.user_id} reply={reply} />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export function TrainRepliesModal({ trainId, isOpen, onClose }: TrainRepliesModalProps) {
  if (!isOpen) return null
  return <ModalContent trainId={trainId} onClose={onClose} />
}
