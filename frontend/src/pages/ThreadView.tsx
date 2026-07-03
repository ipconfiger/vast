import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { useParams, useNavigate } from 'react-router'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Send, Loader2 } from 'lucide-react'
import dayjs from 'dayjs'
import { apiClient, ApiClientError } from '../api/client'
import { useAuthStore } from '../stores/authStore'
import { useMessageStore } from '../stores/messageStore'
import { useWebSocket } from '../hooks/useWebSocket'
import { MessageListSkeleton } from '../components/Skeletons'
import type { Message } from '../types'

interface ThreadResponse {
  messages: Message[]
  next_cursor: number
  has_more: boolean
}

export function ThreadView() {
  const { channelId, messageId } = useParams<{ channelId: string; messageId: string }>()
  const navigate = useNavigate()

  useWebSocket()

  if (!channelId || !messageId) {
    return (
      <div className="flex h-screen items-center justify-center bg-zinc-950 text-zinc-500">
        <button
          onClick={() => navigate('/channels')}
          className="flex items-center gap-2 text-sm hover:text-zinc-300"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to channels
        </button>
      </div>
    )
  }

  return <ThreadContent channelId={channelId} messageId={messageId} />
}

function ThreadContent({ channelId, messageId }: { channelId: string; messageId: string }) {
  const navigate = useNavigate()
  const parentIdNum = Number(messageId)

  const { data, isLoading, error } = useQuery<ThreadResponse>({
    queryKey: ['thread', channelId, messageId],
    queryFn: () =>
      apiClient<ThreadResponse>(
        `/channels/${channelId}/messages/${messageId}/thread`,
      ),
  })

  if (error) {
    return (
      <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
        <ThreadHeader channelId={channelId} />
        <div className="flex flex-1 flex-col items-center justify-center text-center">
          <p className="text-sm text-red-400">
            {error instanceof ApiClientError ? error.message : 'Could not load thread.'}
          </p>
          <button
            onClick={() => navigate(`/channels/${channelId}`)}
            className="mt-4 flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-300"
          >
            <ArrowLeft className="h-4 w-4" />
            Back to channel
          </button>
        </div>
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
        <ThreadHeader channelId={channelId} />
        <MessageListSkeleton />
      </div>
    )
  }

  const replies = data?.messages ?? []

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      <ThreadHeader channelId={channelId} />
      <div className="flex-1 overflow-y-auto">
        <ParentMessage channelId={channelId} messageId={messageId} />
        <div className="border-t border-zinc-800/60 px-6 py-3">
          <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">
            {replies.length} {replies.length === 1 ? 'reply' : 'replies'}
          </h2>
        </div>
        {replies.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-10 text-center">
            <p className="text-sm text-zinc-500">No replies yet</p>
            <p className="mt-1 text-xs text-zinc-600">Be the first to reply in this thread</p>
          </div>
        ) : (
          <ul className="flex flex-col py-1">
            {replies.map((reply) => (
              <ThreadReplyRow key={reply.id || reply.msg_id} message={reply} />
            ))}
          </ul>
        )}
      </div>
      <ThreadReplyInput channelId={channelId} parentId={parentIdNum} />
    </div>
  )
}

function ThreadHeader({ channelId }: { channelId: string }) {
  const navigate = useNavigate()
  return (
    <header className="border-b border-zinc-800 px-6 py-3">
      <button
        onClick={() => navigate(`/channels/${channelId}`)}
        className="mb-2 flex items-center gap-2 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
      >
        <ArrowLeft className="h-3.5 w-3.5" />
        Back to channel
      </button>
      <h1 className="font-semibold text-sm text-zinc-100">Thread</h1>
    </header>
  )
}

function ParentMessage({
  channelId,
  messageId,
}: {
  channelId: string
  messageId: string
}) {
  const me = useAuthStore((s) => s.user)
  // The parent message was visible in the channel list, so it should be cached.
  const parent = useMessageStore((s) => {
    const list = s.messagesByChannel.get(channelId) ?? []
    return list.find((m) => String(m.id) === messageId || m.msg_id === messageId)
  })

  if (!parent) {
    return (
      <div className="px-6 py-4">
        <p className="text-xs text-zinc-600">
          Parent message #{messageId} (not in cache)
        </p>
      </div>
    )
  }

  const text =
    typeof parent.payload === 'string'
      ? parent.payload
      : (parent.payload?.text ?? JSON.stringify(parent.payload))
  const senderLabel = parent.sender_id === me?.id ? 'You' : parent.sender_id.slice(0, 8)

  return (
    <div className="flex gap-3 px-6 py-4">
      <div className="flex-shrink-0 pt-0.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold text-zinc-300">
          {senderLabel.charAt(0).toUpperCase()}
        </div>
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="font-semibold text-sm text-zinc-200">{senderLabel}</span>
          <span className="text-xs text-zinc-500">
            {dayjs(parent.created_at).format('MMM D, h:mm A')}
          </span>
        </div>
        <div className="mt-0.5 whitespace-pre-wrap break-words text-sm text-zinc-100 leading-relaxed">
          {text}
        </div>
      </div>
    </div>
  )
}

function ThreadReplyRow({ message }: { message: Message }) {
  const me = useAuthStore((s) => s.user)
  const text =
    typeof message.payload === 'string'
      ? message.payload
      : (message.payload?.text ?? '')
  const senderLabel = message.sender_id === me?.id ? 'You' : message.sender_id.slice(0, 8)

  return (
    <li className="flex gap-3 px-6 py-2 hover:bg-zinc-800/30">
      <div className="flex-shrink-0 pt-0.5">
        <div className="flex h-8 w-8 items-center justify-center rounded-full bg-zinc-700 text-xs font-semibold text-zinc-300">
          {senderLabel.charAt(0).toUpperCase()}
        </div>
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="font-semibold text-sm text-zinc-200">{senderLabel}</span>
          <span className="text-xs text-zinc-500">
            {dayjs(message.created_at).format('h:mm A')}
          </span>
        </div>
        <div className="mt-0.5 whitespace-pre-wrap break-words text-sm text-zinc-100 leading-relaxed">
          {text}
        </div>
      </div>
    </li>
  )
}

function ThreadReplyInput({
  channelId,
  parentId,
}: {
  channelId: string
  parentId: number
}) {
  const [text, setText] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const queryClient = useQueryClient()

  const send = useMutation({
    mutationFn: (body: {
      msg_type: string
      payload: unknown
      thread_parent_id: number
    }) =>
      apiClient<Message>(`/channels/${channelId}/messages`, {
        method: 'POST',
        body: JSON.stringify(body),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['thread', channelId, String(parentId)],
      })
    },
  })

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 160)}px`
    }
  }, [text])

  const handleSend = () => {
    const trimmed = text.trim()
    if (!trimmed || send.isPending) return
    send.mutate({
      msg_type: 'text',
      payload: { text: trimmed },
      thread_parent_id: parentId,
    })
    setText('')
    if (textareaRef.current) textareaRef.current.style.height = 'auto'
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  return (
    <div className="border-t border-zinc-800 bg-zinc-900/80 px-6 py-3">
      <div className="flex items-end gap-2 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 focus-within:border-zinc-500 transition-colors">
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Reply in thread..."
          rows={1}
          className="flex-1 resize-none bg-transparent text-sm text-zinc-100 placeholder-zinc-500 outline-none"
        />
        <button
          onClick={handleSend}
          disabled={!text.trim() || send.isPending}
          className="flex-shrink-0 rounded-md p-1.5 text-zinc-400 hover:text-zinc-100 hover:bg-zinc-700 disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-zinc-400 transition-colors"
          aria-label="Send reply"
        >
          {send.isPending ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Send className="h-4 w-4" />
          )}
        </button>
      </div>
    </div>
  )
}
