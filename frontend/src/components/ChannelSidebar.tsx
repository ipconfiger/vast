import { useState } from 'react'
import { useNavigate, useParams } from 'react-router'
import { Plus, Hash, Lock, Users, Loader2, Menu, X } from 'lucide-react'
import { useChannels, useCreateChannel } from '../api/channels'
import { useChannelStore } from '../stores/channelStore'
import { usePresenceStore } from '../stores/presenceStore'
import { useUserStore } from '../stores/userStore'
import { ChannelListSkeleton } from './Skeletons'
import { NoChannelsEmpty } from './EmptyState'
import type { Channel } from '../types'

function ChannelIcon({ type }: { type: Channel['type'] }) {
  switch (type) {
    case 'public':
      return <Hash className="h-4 w-4 flex-shrink-0" />
    case 'private':
      return <Lock className="h-4 w-4 flex-shrink-0" />
    case 'dm':
      return <Users className="h-4 w-4 flex-shrink-0" />
  }
}

function ChannelItem({
  channel,
  isActive,
  onClick,
}: {
  channel: Channel
  isActive: boolean
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={`channel-item flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-sm transition-colors ${
        isActive
          ? 'bg-zinc-700/70 text-zinc-100'
          : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200'
      }`}
    >
      <ChannelIcon type={channel.type} />
      <span className="truncate">{channel.name}</span>
    </button>
  )
}

interface ChannelSidebarProps {
  onClose?: () => void
}

export function ChannelSidebar({ onClose }: ChannelSidebarProps) {
  const navigate = useNavigate()
  const { channelId } = useParams<{ channelId: string }>()
  const { isLoading } = useChannels()
  const channels = useChannelStore((s) => s.channels)
  const setCurrentChannel = useChannelStore((s) => s.setCurrentChannel)
  const createChannel = useCreateChannel()
  const onlineUsers = usePresenceStore((s) => s.onlineUsers)
  const getName = useUserStore((s) => s.getName)

  const handleChannelClick = (channel: Channel) => {
    setCurrentChannel(channel.id)
    navigate(`/channels/${channel.id}`)
    onClose?.()
  }

  const handleCreateChannel = () => {
    const name = window.prompt('Channel name:')
    if (!name?.trim()) return
    const description = window.prompt('Description (optional):') ?? undefined
    createChannel.mutate(
      { name: name.trim(), description: description?.trim() || undefined },
      {
        onSuccess: (channel: Channel) => {
          setCurrentChannel(channel.id)
          navigate(`/channels/${channel.id}`)
          onClose?.()
        },
      },
    )
  }

  return (
    <aside className="channel-sidebar flex h-full w-[300px] flex-shrink-0 flex-col border-r border-zinc-800 bg-zinc-950">
      <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
        <h2 className="font-semibold text-sm text-zinc-100">Channels</h2>
        <div className="flex items-center gap-1">
          <button
            onClick={handleCreateChannel}
            disabled={createChannel.isPending}
            className="rounded-md p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200 transition-colors"
            aria-label="Create channel"
          >
            {createChannel.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Plus className="h-4 w-4" />
            )}
          </button>
          {onClose && (
            <button
              onClick={onClose}
              className="rounded-md p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200 transition-colors lg:hidden"
              aria-label="Close sidebar"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-2">
        {isLoading ? (
          <ChannelListSkeleton />
        ) : channels.length === 0 ? (
          <NoChannelsEmpty />
        ) : (
          <div className="flex flex-col gap-0.5">
            {channels.map((channel) => (
              <ChannelItem
                key={channel.id}
                channel={channel}
                isActive={channel.id === channelId}
                onClick={() => handleChannelClick(channel)}
              />
            ))}
          </div>
        )}

        {onlineUsers.size > 0 && (
          <div className="mt-4 border-t border-zinc-800 pt-3">
            <h3 className="px-3 pb-2 text-xs font-semibold text-zinc-500 uppercase tracking-wider">
              Online — {onlineUsers.size}
            </h3>
            <div className="flex flex-col gap-0.5">
              {Array.from(onlineUsers).map((userId) => {
                const name = getName(userId) ?? userId.slice(0, 8)
                return (
                  <div
                    key={userId}
                    className="flex items-center gap-2 rounded-md px-3 py-1.5 text-sm text-zinc-400"
                  >
                    <span className="relative flex h-2 w-2">
                      <span className="absolute inline-flex h-full w-full animate-pulse rounded-full bg-emerald-400 opacity-75" />
                    </span>
                    <span className="truncate">{name}</span>
                  </div>
                )
              })}
            </div>
          </div>
        )}
      </div>
    </aside>
  )
}

export function ChannelSidebarToggle() {
  const [isOpen, setIsOpen] = useState(false)

  return (
    <>
      {/* Mobile hamburger */}
      <button
        onClick={() => setIsOpen(true)}
        className="fixed top-3 left-3 z-40 rounded-md p-2 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors lg:hidden"
        aria-label="Open sidebar"
      >
        <Menu className="h-5 w-5" />
      </button>

      {/* Mobile overlay */}
      {isOpen && (
        <div className="fixed inset-0 z-50 lg:hidden">
          <div
            className="absolute inset-0 bg-black/60 backdrop-blur-sm"
            onClick={() => setIsOpen(false)}
          />
          <div className="absolute left-0 top-0 h-full">
            <ChannelSidebar onClose={() => setIsOpen(false)} />
          </div>
        </div>
      )}

      {/* Desktop sidebar always visible */}
      <div className="hidden lg:block h-full">
        <ChannelSidebar />
      </div>
    </>
  )
}
