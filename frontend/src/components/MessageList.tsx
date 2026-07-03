import { useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { useMessages } from '../api/channels'
import { useMessageStore } from '../stores/messageStore'
import { useAuthStore } from '../stores/authStore'
import { MessageBubble } from './MessageBubble'
import { MessageListSkeleton } from './Skeletons'
import { NoMessagesEmpty } from './EmptyState'
import dayjs from 'dayjs'

interface MessageListProps {
  channelId: string
}

export function MessageList({ channelId }: MessageListProps) {
  const parentRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const prevMessageCountRef = useRef(0)

  const { data: messageData, isLoading } = useMessages(channelId)
  // Keep `?? []` OUTSIDE the selector: returning a fresh `[]` from inside
  // trips React's useSyncExternalStore "getSnapshot should be cached" loop.
  const messages = useMessageStore((s) => s.messagesByChannel.get(channelId)) ?? []
  const setMessages = useMessageStore((s) => s.setMessages)
  const user = useAuthStore((s) => s.user)

  useEffect(() => {
    if (messageData) setMessages(channelId, messageData)
  }, [messageData, channelId, setMessages])

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 68,
    overscan: 5,
  })

  const virtualItems = virtualizer.getVirtualItems()

  useEffect(() => {
    const prevCount = prevMessageCountRef.current
    const newCount = messages.length

    if (newCount > prevCount) {
      const lastMessage = messages[newCount - 1]
      const isOwn = lastMessage && lastMessage.sender_id === user?.id
      const isInitial = prevCount === 0

      if (isOwn || isInitial) {
        virtualizer.scrollToIndex(newCount - 1, { align: 'end' })
      } else {
        const scrollEl = parentRef.current
        if (scrollEl) {
          const isNearBottom = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 200
          if (isNearBottom) {
            virtualizer.scrollToIndex(newCount - 1, { align: 'end' })
          }
        }
      }
    }

    prevMessageCountRef.current = newCount
  }, [messages.length, virtualizer, user?.id])

  useEffect(() => {
    if (messages.length === 0) return
    const timer = setTimeout(() => {
      virtualizer.scrollToIndex(messages.length - 1, { align: 'end' })
    }, 100)
    return () => clearTimeout(timer)
  }, [channelId])

  if (isLoading) {
    return <MessageListSkeleton />
  }

  if (messages.length === 0) {
    return <NoMessagesEmpty />
  }

  return (
    <div ref={parentRef} className="flex-1 overflow-y-auto">
      <div
        className="relative w-full"
        style={{ height: `${virtualizer.getTotalSize()}px` }}
      >
        <div
          className="absolute top-0 left-0 w-full"
          style={{
            transform: `translateY(${virtualItems[0]?.start ?? 0}px)`,
          }}
        >
          {virtualItems.map((virtualItem) => {
            const message = messages[virtualItem.index]
            return (
              <div
                key={message.id || message.msg_id}
                data-index={virtualItem.index}
                ref={virtualizer.measureElement}
              >
                <MessageBubble
                  message={message}
                  isOwn={message.sender_id === user?.id}
                  senderName={message.sender_id === user?.id ? 'You' : (message as any).sender_name || message.sender_id.slice(0, 8)}
                  timestamp={dayjs(message.created_at).format('h:mm A')}
                  channelId={channelId}
                />
              </div>
            )
          })}
        </div>
      </div>
      <div ref={bottomRef} />
    </div>
  )
}
