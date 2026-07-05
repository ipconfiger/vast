import { useState, useEffect } from 'react'
import { useNavigate, useParams } from 'react-router'
import { Plus, Hash, Lock, Users, Loader2, Menu, X, Globe } from 'lucide-react'
import { useChannels, useCreateChannel } from '../api/channels'
import { useDms } from '../api/dm'
import { useChannelStore } from '../stores/channelStore'
import { usePresenceStore } from '../stores/presenceStore'
import { useUserStore } from '../stores/userStore'
import { useAuthStore } from '../stores/authStore'
import { useUnreadStore } from '../stores/unreadStore'
import { useAuthImage } from '../hooks/useAuthImage'
import { ChannelListSkeleton } from './Skeletons'
import { NoChannelsEmpty } from './EmptyState'
import { CreateChannelDialog } from './CreateChannelDialog'
import { DiscoverChannelsModal } from './DiscoverChannelsModal'
import type { Channel } from '../types'
import type { DmChannel } from '../api/dm'

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

export function ChannelItem({
  channel,
  isActive,
  onClick,
}: {
  channel: Channel
  isActive: boolean
  onClick: () => void
}) {
  const unread = useUnreadStore((s) => s.unreadByChannel[channel.id] ?? 0)
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
      {unread > 0 && !isActive && (
        <span className="ml-auto inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 text-xs font-medium text-white bg-red-500 rounded-full">
          {unread > 99 ? '99+' : unread}
        </span>
      )}
    </button>
  )
}

function dmDisplayName(dm: DmChannel, currentUsername: string): string {
  // Backend generates DM names as "user1, user2" — strip self.
  if (!dm.is_direct && !dm.is_group_dm) return dm.name
  const others = dm.name
    .split(',')
    .map((n) => n.trim())
    .filter((n) => n && n !== currentUsername)
  return others.length > 0 ? others.join(', ') : dm.name
}

export function DmItem({
  dm,
  onClick,
}: {
  dm: DmChannel
  onClick: () => void
}) {
  const currentUsername = useAuthStore((s) => s.user?.username ?? '')
  const unread = useUnreadStore((s) => s.unreadByChannel[dm.id] ?? 0)
  return (
    <button
      onClick={onClick}
      className="dm-item flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-sm text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-200"
    >
      <Users className="h-4 w-4 flex-shrink-0" />
      <span className="truncate">{dmDisplayName(dm, currentUsername)}</span>
      {unread > 0 && (
        <span className="ml-auto inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 text-xs font-medium text-white bg-red-500 rounded-full">
          {unread > 99 ? '99+' : unread}
        </span>
      )}
    </button>
  )
}

interface ChannelSidebarProps {
  onClose?: () => void
}

export function ChannelSidebar({ onClose }: ChannelSidebarProps) {
  const navigate = useNavigate()
  const { channelId } = useParams<{ channelId: string }>()
  const { data: channelData, isLoading } = useChannels()
  const { data: dms } = useDms()
  const channels = useChannelStore((s) => s.channels)
  const setChannels = useChannelStore((s) => s.setChannels)
  const setCurrentChannel = useChannelStore((s) => s.setCurrentChannel)
  const createChannel = useCreateChannel()
  const onlineUsers = usePresenceStore((s) => s.onlineUsers)
  const getName = useUserStore((s) => s.getName)
  const currentUserId = useAuthStore((s) => s.user?.id)
  const user = useAuthStore((s) => s.user)
  const avatarSrc = useAuthImage(user?.avatar_url)

  useEffect(() => {
    if (channelData) setChannels(channelData)
  }, [channelData, setChannels])

  const handleChannelClick = (channel: Channel) => {
    setCurrentChannel(channel.id)
    navigate(`/channels/${channel.id}`)
    onClose?.()
  }

  const [isCreateOpen, setIsCreateOpen] = useState(false)
  const [isDiscoverOpen, setIsDiscoverOpen] = useState(false)

  const handleCreateChannel = (data: {
    name: string
    description?: string
  }) => {
    createChannel.mutate(data, {
      onSuccess: (channel: Channel) => {
        setIsCreateOpen(false)
        setCurrentChannel(channel.id)
        navigate(`/channels/${channel.id}`)
        onClose?.()
      },
    })
  }

  return (
    <aside className="channel-sidebar flex h-full w-[300px] flex-shrink-0 flex-col border-r border-zinc-800 bg-zinc-950">
      <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
        <h2 className="font-semibold text-sm text-zinc-100">Channels</h2>
        <div className="flex items-center gap-1">
          <button
            onClick={() => setIsDiscoverOpen(true)}
            className="rounded-md p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200 transition-colors"
            aria-label="Discover channels"
          >
            <Globe className="h-4 w-4" />
          </button>
          <button
            onClick={() => setIsCreateOpen(true)}
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
          <NoChannelsEmpty onBrowse={() => setIsDiscoverOpen(true)} />
        ) : (
          <div className="flex flex-col gap-0.5">
            {channels.filter(c => c.owner_id === currentUserId).map(c => (
              <ChannelItem key={c.id} channel={c} isActive={c.id === channelId} onClick={() => handleChannelClick(c)} />
            ))}
            {channels.filter(c => c.owner_id !== currentUserId).length > 0 && (
              <h3 className="px-3 pt-3 pb-1 text-xs font-semibold text-zinc-500 uppercase tracking-wider">Joined</h3>
            )}
            {channels.filter(c => c.owner_id !== currentUserId).map(c => (
              <ChannelItem key={c.id} channel={c} isActive={c.id === channelId} onClick={() => handleChannelClick(c)} />
            ))}
          </div>
        )}

        {dms && dms.length > 0 && (
          <div className="mt-4 border-t border-zinc-800 pt-3">
            <h3 className="px-3 pb-2 text-xs font-semibold text-zinc-500 uppercase tracking-wider">
              Direct Messages
            </h3>
            <div className="flex flex-col gap-0.5">
              {dms.map((dm) => (
                <DmItem
                  key={dm.id}
                  dm={dm}
                  onClick={() => {
                    navigate(`/channels/${dm.id}`)
                    onClose?.()
                  }}
                />
              ))}
            </div>
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

        <div className="mt-auto border-t border-zinc-800 pt-3 px-3 pb-3">
          <button onClick={() => { navigate('/profile'); onClose?.() }} className="flex items-center gap-2 w-full rounded-md px-3 py-1.5 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors">
            {avatarSrc ? (
              <img src={avatarSrc} className="h-7 w-7 rounded-full object-cover" />
            ) : (
              <div className="flex h-7 w-7 items-center justify-center rounded-full bg-zinc-700 text-xs font-semibold text-zinc-300">
                {user?.username?.charAt(0).toUpperCase() || '?'}
              </div>
            )}
            <span className="truncate">{user?.display_name || user?.username || 'Profile'}</span>
          </button>
        </div>
      </div>

      <CreateChannelDialog
        isOpen={isCreateOpen}
        isPending={createChannel.isPending}
        onClose={() => setIsCreateOpen(false)}
        onCreate={handleCreateChannel}
      />

      <DiscoverChannelsModal
        isOpen={isDiscoverOpen}
        onClose={() => setIsDiscoverOpen(false)}
      />
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
