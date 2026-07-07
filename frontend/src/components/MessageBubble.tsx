import { useState, useEffect } from 'react'
import type { Message } from '../types'
import { FileText, Download, Check, X, UserPlus, Loader2, Bot, Trash2 } from 'lucide-react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '../api/client'
import { useDeleteFile } from '../api/files'
import { useAuthStore } from '../stores/authStore'
import { useAuthImage } from '../hooks/useAuthImage'
import { TextMessage } from './TextMessage'
import { CodeMessage } from './CodeMessage'
import { ReactionPicker } from './ReactionPicker'
import { ReactionBar } from './ReactionBar'
import { TrainMessage } from './TrainMessage'
import { VoteMessage } from './VoteMessage'
import { UserAvatar } from './UserAvatar'

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}

function FileMessage({
  payload,
  message,
}: {
  payload?: {
    file_id?: string
    url?: string
    original_name?: string
    size?: number
    mime_type?: string
  }
  message: Message
}) {
  const fileId = payload?.file_id
  const currentUserId = useAuthStore((s) => s.user?.id)
  const deleteFile = useDeleteFile()
  const [isDeleted, setIsDeleted] = useState(false)

  const name = payload?.original_name || 'Unknown file'
  const url = payload?.url || '#'
  const isImage = payload?.mime_type?.startsWith('image/')
  // blob URL + URL.revokeObjectURL on unmount are owned by useAuthImage
  const imgSrc = useAuthImage(isImage && url !== '#' && !isDeleted ? url : null)

  useEffect(() => {
    if (!fileId) return
    let cancelled = false
    const token = useAuthStore.getState().token
    const controller = new AbortController()

    fetch(`/api/files/${fileId}`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      signal: controller.signal,
    })
      .then((res) => {
        if (cancelled) return
        setIsDeleted(res.status === 410)
        controller.abort()
      })
      .catch((err) => {
        if (cancelled) return
        if (err.name !== 'AbortError') {
          setIsDeleted(false)
        }
      })

    return () => {
      cancelled = true
      controller.abort()
    }
  }, [fileId])

  useEffect(() => {
    if (!fileId) return
    const handler = (e: Event) => {
      const ce = e as CustomEvent<{ file_id: string; channel_id: string }>
      if (ce.detail?.file_id === fileId) {
        setIsDeleted(true)
      }
    }
    window.addEventListener('file-deleted', handler)
    return () => window.removeEventListener('file-deleted', handler)
  }, [fileId])

  if (isDeleted) {
    return (
      <div className="inline-flex items-center gap-3 rounded-lg border border-zinc-700/50 bg-zinc-800/30 p-3 max-w-sm opacity-60">
        <FileText className="h-8 w-8 text-zinc-500 flex-shrink-0" />
        <div className="min-w-0 flex-1">
          <p className="text-sm text-zinc-500 truncate">{name}</p>
          <p className="text-xs text-zinc-600">该文件已被发布者删除</p>
        </div>
      </div>
    )
  }

  if (isImage) {
    return (
      <a href={url} target="_blank" rel="noopener noreferrer">
        {imgSrc ? (
          <img
            src={imgSrc}
            alt={name}
            className="inline-block max-w-md max-h-96 rounded-lg border border-zinc-700"
          />
        ) : (
          <div className="inline-flex items-center gap-2 rounded-lg border border-zinc-700 bg-zinc-800/50 p-3 max-w-sm">
            <Loader2 className="h-5 w-5 animate-spin text-zinc-400" />
            <span className="text-sm text-zinc-400">Loading...</span>
          </div>
        )}
      </a>
    )
  }

  return (
    <div className="file-message inline-flex items-center gap-3 rounded-lg border border-zinc-700 bg-zinc-800/50 p-3 max-w-sm">
      <FileText className="h-8 w-8 text-zinc-400 flex-shrink-0" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-zinc-200 truncate">{name}</p>
        {payload?.size && (
          <p className="text-xs text-zinc-500">{formatSize(payload.size)}</p>
        )}
      </div>
      {currentUserId === message.sender_id && fileId && (
        <button
          onClick={async (e) => {
            e.preventDefault()
            if (confirm('确定要删除这个文件吗？')) {
              await deleteFile.mutateAsync(fileId)
              setIsDeleted(true)
            }
          }}
          className="flex-shrink-0 p-1.5 rounded-md text-zinc-400 hover:text-red-400 hover:bg-red-500/10"
          disabled={deleteFile.isPending}
          title="删除文件"
        >
          <Trash2 className="h-4 w-4" />
        </button>
      )}
      <a
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        className="flex-shrink-0 p-1.5 rounded-md text-zinc-400 hover:text-zinc-100 hover:bg-zinc-700"
      >
        <Download className="h-4 w-4" />
      </a>
    </div>
  )
}

function CommandResult({ text }: { text?: string }) {
  return (
    <div className="rounded-lg border border-indigo-500/20 bg-indigo-500/5 px-3 py-2">
      <pre className="text-xs text-indigo-300 whitespace-pre-wrap font-mono leading-relaxed">{text || ''}</pre>
    </div>
  )
}

function JoinRequestMessage({
  payload,
  channelId,
  isOwn,
}: {
  payload?: { request_id?: string; username?: string; status?: string }
  channelId: string
  isOwn: boolean
}) {
  const queryClient = useQueryClient()

  const approve = useMutation({
    mutationFn: () =>
      apiClient(`/requests/${payload?.request_id}/approve`, { method: 'PUT' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['messages', channelId] })
      queryClient.invalidateQueries({ queryKey: ['channels'] })
      queryClient.invalidateQueries({ queryKey: ['discover-channels'] })
    },
  })

  const reject = useMutation({
    mutationFn: () =>
      apiClient(`/requests/${payload?.request_id}/reject`, { method: 'PUT' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['messages', channelId] })
    },
  })

  const isApproved = payload?.status === 'approved'
  const isRejected = payload?.status === 'rejected'

  return (
    <div className="flex items-center gap-3 rounded-lg border border-zinc-700/50 bg-zinc-800/30 px-3 py-2">
      <UserPlus className="h-5 w-5 text-indigo-400 flex-shrink-0" />
      <div className="flex-1 min-w-0">
        <p className="text-sm text-zinc-300">
          <span className="font-medium text-zinc-200">@{payload?.username || 'Unknown'}</span>
          {isApproved
            ? ' has been approved'
            : isRejected
              ? ' was rejected'
              : ' requested to join'}
        </p>
      </div>
      {!isApproved && !isRejected && !isOwn && (
        <div className="flex items-center gap-1 flex-shrink-0">
          <button
            onClick={() => approve.mutate()}
            disabled={approve.isPending}
            className="rounded-md p-1.5 text-emerald-400 hover:bg-emerald-500/20 hover:text-emerald-300 disabled:opacity-50 transition-colors"
            aria-label="Approve join request"
          >
            {approve.isPending ? (
              <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
            ) : (
              <Check className="h-4 w-4" />
            )}
          </button>
          <button
            onClick={() => reject.mutate()}
            disabled={reject.isPending}
            className="rounded-md p-1.5 text-red-400 hover:bg-red-500/20 hover:text-red-300 disabled:opacity-50 transition-colors"
            aria-label="Reject join request"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      )}
    </div>
  )
}

interface MessageBubbleProps {
  message: Message
  isOwn: boolean
  senderName: string
  senderAvatar?: string
  timestamp: string
  channelId: string
}

export function MessageBubble({
  message,
  isOwn,
  senderName,
  senderAvatar,
  timestamp,
  channelId,
}: MessageBubbleProps) {
  const isBotMsg = message.is_bot === true && !isOwn

  const renderContent = () => {
    if (message.payload?._train) {
      return (
        <div className={isOwn ? 'flex justify-end' : ''}>
          <TrainMessage
            trainId={message.payload.train_id}
            title={message.payload.title}
            channelId={message.channel_id}
          />
        </div>
      )
    }
    if (message.payload?._vote) {
      return (
        <div className={isOwn ? 'flex justify-end' : ''}>
          <VoteMessage
            voteId={message.payload.vote_id}
            title={message.payload.title}
            channelId={message.channel_id}
          />
        </div>
      )
    }
    switch (message.msg_type) {
      case 'text':
        if (message.payload?._join_request) {
          if (isOwn) return null
          return <JoinRequestMessage payload={message.payload} channelId={channelId} isOwn={isOwn} />
        }
        if (message.payload?._command_result) {
          if (message.payload?._owner_only && !isOwn) return null
          return <CommandResult text={message.payload.text} />
        }
        return <TextMessage text={typeof message.payload === 'string' ? message.payload : message.payload?.text ?? ''} />
      case 'file':
        return <FileMessage payload={message.payload} message={message} />
      case 'code':
        return <CodeMessage language={message.payload?.language ?? 'plaintext'} code={message.payload?.code ?? ''} filename={message.payload?.filename} />
      default:
        return <TextMessage text={typeof message.payload === 'string' ? message.payload : JSON.stringify(message.payload)} />
    }
  }

  return (
    <div className={`message-bubble group flex gap-3 px-4 py-2 hover:bg-zinc-800/30 ${
      isOwn
        ? 'flex-row-reverse'
        : isBotMsg
          ? 'bg-indigo-950/30 border-l-2 border-indigo-500/40'
          : ''
    }`}>
      {!isOwn && (
        <div className="flex-shrink-0 pt-0.5">
          <UserAvatar
            avatarUrl={senderAvatar}
            displayName={senderName}
            size="sm"
            rounded="md"
          />
        </div>
      )}
      <div className={`min-w-0 ${isOwn ? 'flex-1' : 'flex-1'}`}>
        <div className={`flex items-center gap-2 ${isOwn ? 'flex-row-reverse' : ''}`}>
          <div className="flex items-baseline gap-2 min-w-0">
            {isBotMsg && (
              <Bot
                className="h-3.5 w-3.5 text-indigo-400 flex-shrink-0 self-center"
                aria-label="Bot"
              />
            )}
            <span className={`font-semibold text-sm ${isBotMsg ? 'text-indigo-400' : 'text-zinc-200'}`}>
              {senderName}
            </span>
            <span className="text-xs text-zinc-500 opacity-0 group-hover:opacity-100 transition-opacity">
              {timestamp}
            </span>
          </div>
          <div className="ml-auto flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
            <ReactionPicker messageId={message.id || message.msg_id} isOwn={isOwn} />
          </div>
        </div>
        <div className={`mt-0.5 text-sm leading-relaxed ${isOwn ? 'text-right' : 'text-left'}`}>
          {renderContent()}
        </div>
        <div className={isOwn ? 'flex justify-end' : ''}>
          <ReactionBar messageId={message.id || message.msg_id} />
        </div>
      </div>
    </div>
  )
}
