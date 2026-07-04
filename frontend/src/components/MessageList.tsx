import { useCallback, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
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
  const parentRef = useRef<HTMLDivElement>(null)
  const prevMessageCountRef = useRef(0)
  const isAtBottomRef = useRef(true)

  const { data: messageData, isLoading } = useMessages(channelId)
  // Keep `?? []` OUTSIDE the selector: returning a fresh `[]` from inside
  // trips React's useSyncExternalStore "getSnapshot should be cached" loop.
  const messages = useMessageStore((s) => s.messagesByChannel.get(channelId)) ?? []
  const setMessages = useMessageStore((s) => s.setMessages)
  const user = useAuthStore((s) => s.user)

  // Deviates from plan CHANGE 6: the unwrapped Message[] is already bound to
  // `messageData` here (useMessages' queryFn returns data.messages), so the
  // .data member does not exist on it. Intent (depend on data) is met.
  useEffect(() => {
    if (messageData) {
      setMessages(channelId, messageData)
    }
  }, [messageData, channelId, setMessages])

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 68,
    overscan: 5,
    getItemKey: (index) => messages[index]?.id ?? messages[index]?.msg_id ?? index,
  })

  const virtualItems = virtualizer.getVirtualItems()

  const scrollToBottom = useCallback((smooth = false) => {
    requestAnimationFrame(() => {
      virtualizer.scrollToIndex(messages.length - 1, {
        align: 'end',
        behavior: smooth ? 'smooth' : 'auto',
      })
    })
  }, [virtualizer, messages.length])

  useEffect(() => {
    const prevCount = prevMessageCountRef.current
    const newCount = messages.length

    if (newCount > prevCount) {
      const lastMessage = messages[newCount - 1]
      const isOwn = lastMessage && lastMessage.sender_id === user?.id
      const isInitial = prevCount === 0

      if (isOwn || isInitial || isAtBottomRef.current) {
        scrollToBottom()
      }
    }

    prevMessageCountRef.current = newCount
  }, [messages.length, user?.id, scrollToBottom])

  useEffect(() => {
    if (messages.length === 0) return
    isAtBottomRef.current = true
    const timer = setTimeout(() => {
      scrollToBottom(true)
    }, 100)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channelId])

  // Cannot use [] here: the loading early-return renders <MessageListSkeleton/>
  // before parentRef is bound, so a mount-only effect would bail on null forever.
  useEffect(() => {
    const el = parentRef.current
    if (!el) return
    const onScroll = () => {
      isAtBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80
    }
    el.addEventListener('scroll', onScroll, { passive: true })
    return () => el.removeEventListener('scroll', onScroll)
  }, [messages.length])

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
                  senderAvatar={message.sender_id === user?.id ? user?.avatar_url : message.sender_avatar_url}
                  senderName={
                    message.sender_id === user?.id
                      ? 'You'
                      : getUserDisplayName(message.sender_display_name, message.sender_name, message.sender_id)
                  }
                  timestamp={dayjs(message.created_at).format('h:mm A')}
                  channelId={channelId}
                />
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
