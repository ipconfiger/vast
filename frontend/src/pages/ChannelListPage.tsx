import { useEffect, useState } from 'react'
import { Settings, Loader2 } from 'lucide-react'
import { useParams, useNavigate, useLocation } from 'react-router'
import { ChannelSettingsModal } from '../components/ChannelSettingsModal'
import { ChannelSidebarToggle } from '../components/ChannelSidebar'
import { MessageList } from '../components/MessageList'
import { MessageInput } from '../components/MessageInput'
import { TypingIndicator } from '../components/TypingIndicator'
import { useAuthStore } from '../stores/authStore'
import { useChannelStore } from '../stores/channelStore'
import { useChannel, downloadChannelArchive } from '../api/channels'
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

  const location = useLocation()

  useEffect(() => {
    if (!channelId && location.pathname !== '/channels') {
      navigate('/channels', { replace: true })
    }
  }, [channelId, navigate, location.pathname])

  const { data: channel, isLoading: channelLoading } = useChannel(channelId ?? null)
  const user = useAuthStore((s) => s.user)
  const isOwner = channel?.owner_id === user?.id
  const [showSettings, setShowSettings] = useState(false)

  return (
    <div className="channel-page flex h-screen bg-zinc-950 text-zinc-100">
      <ChannelSidebarToggle />
      <main className="flex flex-1 flex-col min-w-0">
        {channelId ? (
          channelLoading ? (
            <div className="flex items-center justify-center h-full">
              <Loader2 className="h-6 w-6 animate-spin text-zinc-600" />
            </div>
          ) : channel?.is_archived ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-4">
              <div className="text-center">
                <h2 className="text-xl font-semibold text-zinc-300 mb-2">
                  # {channel?.name ?? channelId.slice(0, 8)}
                  <span className="ml-2 text-sm text-zinc-500 font-normal">[Archived]</span>
                </h2>
                <p className="text-zinc-500 text-sm mb-6">This channel has been archived.</p>
                <div className="flex gap-3 justify-center">
                  <button onClick={() => {
                      if (!channel) return
                      downloadChannelArchive(channelId, channel.name).catch((err) => {
                        console.error('Archive download failed:', err)
                      })
                    }}
                    className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 transition-colors">
                    Download Archive
                  </button>
                  <button onClick={() => navigate('/channels')}
                    className="rounded-lg border border-zinc-700 px-4 py-2 text-sm text-zinc-400 hover:bg-zinc-800 transition-colors">
                    Back to Channels
                  </button>
                </div>
              </div>
              {isOwner && (
                <button onClick={() => setShowSettings(true)}
                  className="absolute top-3 right-3 rounded-md p-1 text-zinc-500 hover:text-zinc-300 transition-colors">
                  <Settings className="h-4 w-4" />
                </button>
              )}
              {showSettings && isOwner && (
                <ChannelSettingsModal channelId={channelId} isOpen={showSettings} onClose={() => setShowSettings(false)} />
              )}
            </div>
          ) : (
            <>
              <ChannelHeader channelId={channelId} />
              <MessageList channelId={channelId} />
              <TypingIndicator channelId={channelId} />
              <MessageInput channelId={channelId} currentRole={channel?.role} />
            </>
          )
        ) : (
          <SelectChannelPrompt />
        )}
      </main>
    </div>
  )
}

function ChannelHeader({ channelId }: { channelId: string }) {
  const { data: channel, isLoading } = useChannel(channelId)
  const user = useAuthStore((s) => s.user)
  const [showSettings, setShowSettings] = useState(false)

  const isOwner = channel?.owner_id === user?.id

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 border-b border-zinc-800 px-6 py-3">
        <div className="h-4 w-32 animate-pulse rounded bg-zinc-800" />
      </div>
    )
  }

  return (
    <>
      <div className="flex items-center gap-2 border-b border-zinc-800 px-6 py-3">
        <h1 className="font-semibold text-sm text-zinc-100">
          {channel?.name ? `# ${channel.name}` : `# ${channelId.slice(0, 8)}`}
        </h1>
        {channel?.description && (
          <span className="text-xs text-zinc-500">— {channel.description}</span>
        )}
        {isOwner && (
          <button
            onClick={() => setShowSettings(true)}
            title="Channel settings"
            aria-label="Channel settings"
            className="ml-auto rounded-md p-1 text-zinc-500 hover:text-zinc-300 transition-colors"
          >
            <Settings className="h-4 w-4" />
          </button>
        )}
      </div>
      {showSettings && isOwner && (
        <ChannelSettingsModal
          channelId={channelId}
          isOpen={showSettings}
          onClose={() => setShowSettings(false)}
        />
      )}
    </>
  )
}
