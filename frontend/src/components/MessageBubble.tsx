import type { Message } from '../types'
import { useState, useEffect } from 'react'
import { FileText, Download, Check, X, UserPlus, Loader2 } from 'lucide-react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '../api/client'
import { useAuthStore } from '../stores/authStore'
import { TextMessage } from './TextMessage'
import { CodeMessage } from './CodeMessage'
import { ReactionPicker } from './ReactionPicker'
import { ReactionBar } from './ReactionBar'

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}

function FileMessage({
  payload,
}: {
  payload?: {
    file_id?: string
    url?: string
    original_name?: string
    size?: number
    mime_type?: string
  }
}) {
  const name = payload?.original_name || 'Unknown file'
  const url = payload?.url || '#'
  const isImage = payload?.mime_type?.startsWith('image/')
  const token = useAuthStore((s) => s.token)
  const [imgSrc, setImgSrc] = useState<string | null>(null)

  useEffect(() => {
    if (!isImage || !url || !token || url === '#') return
    let cancelled = false
    fetch(url, { headers: { Authorization: `Bearer ${token}` } })
      .then((r) => r.blob())
      .then((blob) => {
        if (!cancelled) setImgSrc(URL.createObjectURL(blob))
      })
      .catch(() => {})
    return () => { cancelled = true }
  }, [url, token, isImage])

  if (isImage) {
    return (
      <a href={url} target="_blank" rel="noopener noreferrer">
        {imgSrc ? (
          <img
            src={imgSrc}
            alt={name}
            className="max-w-md max-h-96 rounded-lg border border-zinc-700"
          />
        ) : (
          <div className="flex items-center gap-2 rounded-lg border border-zinc-700 bg-zinc-800/50 p-3 max-w-sm">
            <Loader2 className="h-5 w-5 animate-spin text-zinc-400" />
            <span className="text-sm text-zinc-400">Loading...</span>
          </div>
        )}
      </a>
    )
  }

  return (
    <div className="file-message flex items-center gap-3 rounded-lg border border-zinc-700 bg-zinc-800/50 p-3 max-w-sm">
      <FileText className="h-8 w-8 text-zinc-400 flex-shrink-0" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-zinc-200 truncate">{name}</p>
        {payload?.size && (
          <p className="text-xs text-zinc-500">{formatSize(payload.size)}</p>
        )}
      </div>
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
  const renderContent = () => {
    switch (message.msg_type) {
      case 'text':
        if (message.payload?._join_request) {
          if (isOwn) return null
          return <JoinRequestMessage payload={message.payload} channelId={channelId} isOwn={isOwn} />
        }
        return <TextMessage text={typeof message.payload === 'string' ? message.payload : message.payload?.text ?? ''} />
      case 'file':
        return <FileMessage payload={message.payload} />
      case 'code':
        return <CodeMessage language={message.payload?.language ?? 'plaintext'} code={message.payload?.code ?? ''} filename={message.payload?.filename} />
      default:
        return <TextMessage text={typeof message.payload === 'string' ? message.payload : JSON.stringify(message.payload)} />
    }
  }

  return (
    <div className={`message-bubble group flex gap-3 px-4 py-2 hover:bg-zinc-800/30 ${isOwn ? 'flex-row-reverse' : ''}`}>
      {!isOwn && (
        <div className="flex-shrink-0 pt-0.5">
          {senderAvatar ? (
            <img
              src={senderAvatar}
              alt={senderName}
              className="h-9 w-9 rounded-md object-cover"
            />
          ) : (
            <div className="flex h-9 w-9 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold text-zinc-300">
              {senderName.charAt(0).toUpperCase()}
            </div>
          )}
        </div>
      )}
      <div className={`min-w-0 ${isOwn ? 'flex-1' : 'flex-1'}`}>
        <div className={`flex items-center gap-2 ${isOwn ? 'flex-row-reverse' : ''}`}>
          <div className="flex items-baseline gap-2 min-w-0">
            <span className="font-semibold text-sm text-zinc-200">
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
        <ReactionBar messageId={message.id || message.msg_id} />
      </div>
    </div>
  )
}
