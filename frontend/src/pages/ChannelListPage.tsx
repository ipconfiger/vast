import { useEffect } from 'react'
import { useParams, useNavigate } from 'react-router'
import { ChannelSidebarToggle } from '../components/ChannelSidebar'
import { MessageList } from '../components/MessageList'
import { MessageInput } from '../components/MessageInput'
import { TypingIndicator } from '../components/TypingIndicator'
import { useChannelStore } from '../stores/channelStore'
import { useChannel } from '../api/channels'
import { useWebSocket } from '../hooks/useWebSocket'
import { useCursorSync } from '../hooks/useCursorSync'
import { SelectChannelPrompt } from '../components/EmptyState'

export function ChannelListPage() {
  const { channelId } = useParams<{ channelId: string }>()
  const navigate = useNavigate()
  const setCurrentChannel = useChannelStore((s) => s.setCurrentChannel)

  useWebSocket()
  useCursorSync()

  useEffect(() => {
    if (channelId) {
      setCurrentChannel(channelId)
    } else {
      setCurrentChannel(null)
    }
  }, [channelId, setCurrentChannel])

  useEffect(() => {
    if (!channelId) {
      navigate('/channels', { replace: true })
    }
  }, [channelId, navigate])

  return (
    <div className="channel-page flex h-screen bg-zinc-950 text-zinc-100">
      <ChannelSidebarToggle />
      <main className="flex flex-1 flex-col min-w-0">
        {channelId ? (
          <>
            <ChannelHeader channelId={channelId} />
            <MessageList channelId={channelId} />
            <TypingIndicator channelId={channelId} />
            <MessageInput channelId={channelId} />
          </>
        ) : (
          <SelectChannelPrompt />
        )}
      </main>
    </div>
  )
}

function ChannelHeader({ channelId }: { channelId: string }) {
  const { data: channel, isLoading } = useChannel(channelId)

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 border-b border-zinc-800 px-6 py-3">
        <div className="h-4 w-32 animate-pulse rounded bg-zinc-800" />
      </div>
    )
  }

  return (
    <div className="flex items-center gap-2 border-b border-zinc-800 px-6 py-3">
      <h1 className="font-semibold text-sm text-zinc-100">
        {channel?.name ? `# ${channel.name}` : `# ${channelId.slice(0, 8)}`}
      </h1>
      {channel?.description && (
        <span className="text-xs text-zinc-500">— {channel.description}</span>
      )}
    </div>
  )
}
