import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { useParams, useNavigate } from 'react-router'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Send, Loader2, User } from 'lucide-react'
import { apiClient, ApiClientError } from '../api/client'
import { useAuthStore } from '../stores/authStore'
import { useMessages } from '../api/channels'
import { useMessageStore } from '../stores/messageStore'
import { useWebSocket } from '../hooks/useWebSocket'
import { useUserStore } from '../stores/userStore'
import { MessageListSkeleton } from '../components/Skeletons'
import dayjs from 'dayjs'
import type { Message } from '../types'

interface DmChannel {
  id: string
  name: string
  is_direct: boolean
  is_group_dm: boolean
  created_at: number
}

export function DirectMessagePage() {
  const { userId } = useParams<{ userId: string }>()
  const navigate = useNavigate()
  const me = useAuthStore((s) => s.user)
  const getName = useUserStore((s) => s.getName)
  const queryClient = useQueryClient()

  const {
    data: dmChannel,
    isLoading,
    error,
  } = useQuery<DmChannel>({
    queryKey: ['dm', userId],
    queryFn: async () => {
      const data = await apiClient<DmChannel>('/dm', {
        method: 'POST',
        body: JSON.stringify({ user_ids: [me!.id, userId!] }),
      })
      queryClient.invalidateQueries({ queryKey: ['dms'] })
      return data
    },
    enabled: !!userId && !!me,
  })

  if (!userId || !me) {
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

  const otherName = getName(userId) ?? userId.slice(0, 8)

  if (error) {
    return (
      <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
        <DmHeader name={otherName} />
        <div className="flex flex-1 flex-col items-center justify-center text-center">
          <p className="text-sm text-red-400">
            {error instanceof ApiClientError ? error.message : 'Could not open DM.'}
          </p>
          <button
            onClick={() => navigate('/channels')}
            className="mt-4 flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-300"
          >
            <ArrowLeft className="h-4 w-4" />
            Back to channels
          </button>
        </div>
      </div>
    )
  }

  if (isLoading || !dmChannel) {
    return (
      <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
        <DmHeader name={otherName} />
        <MessageListSkeleton />
      </div>
    )
  }

  return <DmConversation channelId={dmChannel.id} otherUserId={userId} otherName={otherName} />
}

function DmConversation({
  channelId,
  otherUserId,
  otherName,
}: {
  channelId: string
  otherUserId: string
  otherName: string
}) {
  // Reuse the same channel-message flow as channels — DMs are channels internally.
  // Pre-populate so useMessages populates the store with this DM's messages.
  useMessages(channelId)
  useWebSocket()

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      <DmHeader name={otherName} channelId={channelId} otherUserId={otherUserId} />
      <div className="flex-1 overflow-y-auto">
        <DmMessageList channelId={channelId} otherUserId={otherUserId} otherName={otherName} />
      </div>
      <DmInput channelId={channelId} placeholder={`Message ${otherName}`} />
    </div>
  )
}

function DmHeader({
  name,
  channelId,
  otherUserId,
}: {
  name: string
  channelId?: string
  otherUserId?: string
}) {
  const navigate = useNavigate()
  const back = () => navigate('/channels')
  return (
    <header className="border-b border-zinc-800 px-6 py-3">
      <button
        onClick={back}
        className="mb-2 flex items-center gap-2 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
      >
        <ArrowLeft className="h-3.5 w-3.5" />
        Back
      </button>
      <div className="flex items-center gap-2">
        <div className="flex h-6 w-6 items-center justify-center rounded-full bg-zinc-700 text-xs font-semibold text-zinc-300">
          <User className="h-3.5 w-3.5" />
        </div>
        <h1 className="font-semibold text-sm text-zinc-100">
          {name}
          {channelId && (
            <span className="ml-2 text-xs font-normal text-zinc-600">
              {otherUserId?.slice(0, 8)}
            </span>
          )}
        </h1>
      </div>
    </header>
  )
}

function DmMessageList({
  channelId,
  otherUserId,
  otherName,
}: {
  channelId: string
  otherUserId: string
  otherName: string
}) {
  const me = useAuthStore((s) => s.user)
  // Keep `?? []` OUTSIDE the selector — see MessageList.tsx for rationale.
  const messages: Message[] = useMessageStore((s) => s.messagesByChannel.get(channelId)) ?? []

  if (messages.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <User className="mb-3 h-10 w-10 text-zinc-700" />
        <p className="text-sm text-zinc-500">This is the start of your DM with {otherName}</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col py-2">
      {messages.map((message) => (
        <DmMessageRow
          key={message.id || message.msg_id}
          message={message}
          isOwn={message.sender_id === me?.id}
          isOther={message.sender_id === otherUserId}
          otherName={otherName}
        />
      ))}
    </div>
  )
}

function DmMessageRow({
  message,
  isOwn,
  isOther,
  otherName,
}: {
  message: Message
  isOwn: boolean
  isOther: boolean
  otherName: string
}) {
  const text =
    typeof message.payload === 'string'
      ? message.payload
      : (message.payload?.text ?? '')
  const senderLabel = isOwn ? 'You' : isOther ? otherName : message.sender_id.slice(0, 8)

  return (
    <div className="flex gap-3 px-6 py-2 hover:bg-zinc-800/30">
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
    </div>
  )
}

function DmInput({ channelId, placeholder }: { channelId: string; placeholder: string }) {
  const [text, setText] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const queryClient = useQueryClient()

  const send = useMutation({
    mutationFn: (body: { msg_type: string; payload: unknown }) =>
      apiClient<Message>(`/channels/${channelId}/messages`, {
        method: 'POST',
        body: JSON.stringify(body),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['messages', channelId] })
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
    send.mutate({ msg_type: 'text', payload: { text: trimmed } })
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
          placeholder={placeholder}
          rows={1}
          className="flex-1 resize-none bg-transparent text-sm text-zinc-100 placeholder-zinc-500 outline-none"
        />
        <button
          onClick={handleSend}
          disabled={!text.trim() || send.isPending}
          className="flex-shrink-0 rounded-md p-1.5 text-zinc-400 hover:text-zinc-100 hover:bg-zinc-700 disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-zinc-400 transition-colors"
          aria-label="Send message"
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
