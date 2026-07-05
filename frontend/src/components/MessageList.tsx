import { useEffect, useRef } from 'react'
import { Virtuoso, type VirtuosoHandle } from 'react-virtuoso'
import { useMessages } from '../api/channels'
import { useMessageStore } from '../stores/messageStore'
import { useAuthStore } from '../stores/authStore'
import { MessageBubble } from './MessageBubble'
import { MessageListSkeleton } from './Skeletons'
import { NoMessagesEmpty } from './EmptyState'
import dayjs from 'dayjs'
import { getUserDisplayName } from '../utils/user'

interface MessageListProps {
  channelId: string
}

export function MessageList({ channelId }: MessageListProps) {
  const virtuosoRef = useRef<VirtuosoHandle>(null)
  const prevMessageCountRef = useRef(0)
  const isAtBottomRef = useRef(true)

  const { data: messageData, isLoading } = useMessages(channelId)
  const messages = useMessageStore((s) => s.messagesByChannel.get(channelId)) ?? []

  const setMessages = useMessageStore((s) => s.setMessages)
  const user = useAuthStore((s) => s.user)

  // ── Data flow: sync useMessages (React Query) → messageStore ──
  useEffect(() => {
    if (!messageData) return

    const existing = useMessageStore.getState().messagesByChannel.get(channelId)

    // Initial load — use full replacement.
    if (!existing || existing.length === 0) {
      setMessages(channelId, messageData)
      return
    }

    // Same count — full replacement to catch in-place payload mutations
    // (e.g. join-request status: pending → approved). The old "skip if
    // same last msg_id" optimization missed payload-only updates.
    if (existing.length === messageData.length) {
      setMessages(channelId, messageData)
      return
    }

    // More messages from the server — add only the new ones incrementally.
    if (messageData.length > existing.length) {
      const existingIds = new Set(existing.map(m => m.msg_id))
      const store = useMessageStore.getState()
      for (const msg of messageData) {
        if (!existingIds.has(msg.msg_id)) {
          store.addMessage(channelId, msg)
        }
      }
      return
    }

    // Edge case: fewer messages or reordered — full replacement.
    setMessages(channelId, messageData)
  }, [messageData, channelId, setMessages])

  // ── Scroll: auto-scroll to bottom on new messages ──
  useEffect(() => {
    const prevCount = prevMessageCountRef.current
    const newCount = messages.length

    if (newCount > prevCount) {
      const lastMessage = messages[newCount - 1]
      const isOwn = lastMessage && lastMessage.sender_id === user?.id
      const isInitial = prevCount === 0

      if (isOwn || isInitial || isAtBottomRef.current) {
        requestAnimationFrame(() => {
          virtuosoRef.current?.scrollToIndex({
            index: 'LAST',
            align: 'end',
            behavior: isInitial ? 'auto' : 'smooth',
          })
        })
      }
    }

    prevMessageCountRef.current = newCount
  }, [messages.length, user?.id])

  // ── Channel switch: reset at-bottom flag ──
  useEffect(() => {
    isAtBottomRef.current = true
  }, [channelId])

  if (isLoading) {
    return <MessageListSkeleton />
  }

  if (messages.length === 0) {
    return <NoMessagesEmpty />
  }

  return (
    <Virtuoso
      ref={virtuosoRef}
      data={messages}
      computeItemKey={(_, msg) => msg.id || msg.msg_id}
      itemContent={(_, message) => (
        <MessageBubble
          message={message}
          isOwn={message.sender_id === user?.id}
          senderAvatar={message.sender_id === user?.id ? user?.avatar_url : message.sender_avatar_url}
          senderName={
            message.sender_id === user?.id
              ? 'You'
              : getUserDisplayName(message.sender_display_name, message.sender_name, message.sender_id)
          }
          timestamp={dayjs(message.created_at).format('h:mm A')}
          channelId={channelId}
        />
      )}
      atBottomStateChange={(atBottom) => {
        isAtBottomRef.current = atBottom
      }}
      atBottomThreshold={80}
      alignToBottom
      initialTopMostItemIndex={{ index: 'LAST', align: 'end' }}
      style={{ flex: 1 }}
      data-testid="message-list-scroll"
    />
  )
}
