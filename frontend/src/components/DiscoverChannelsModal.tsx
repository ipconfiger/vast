import { useState } from 'react'
import { X, Search, Users, Globe, Loader2 } from 'lucide-react'
import { useDiscoverChannels, useJoinChannel } from '../api/channels'

interface DiscoverChannelsModalProps {
  isOpen: boolean
  onClose: () => void
}

export function DiscoverChannelsModal({
  isOpen,
  onClose,
}: DiscoverChannelsModalProps) {
  const [search, setSearch] = useState('')
  const { data, isLoading, isError } = useDiscoverChannels()
  const joinChannel = useJoinChannel()

  if (!isOpen) return null

  const channels = data?.channels ?? []

  const filtered = search.trim()
    ? channels.filter(
        (c) =>
          c.name.toLowerCase().includes(search.toLowerCase()) ||
          c.description.toLowerCase().includes(search.toLowerCase()),
      )
    : channels

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative flex max-h-[80vh] w-full max-w-lg flex-col rounded-2xl border border-zinc-800 bg-zinc-950 shadow-2xl shadow-black/50">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <div className="flex items-center gap-2">
            <Globe className="h-5 w-5 text-zinc-400" />
            <h2 className="text-lg font-semibold text-zinc-100">
              Discover Channels
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-zinc-500 hover:text-zinc-300 transition-colors"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Search */}
        <div className="border-b border-zinc-800 px-6 py-3">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-zinc-500" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search channels..."
              className="w-full rounded-lg border border-zinc-700 bg-zinc-800 py-2 pl-9 pr-3 text-sm text-zinc-100 placeholder-zinc-500 transition-all focus:border-indigo-500/50 focus:outline-none focus:ring-2 focus:ring-indigo-500/50"
            />
          </div>
        </div>

        {/* Channel list */}
        <div className="flex-1 overflow-y-auto px-6 py-3">
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="h-6 w-6 animate-spin text-zinc-500" />
            </div>
          ) : isError ? (
            <div className="py-8 text-center text-sm text-zinc-500">
              Failed to load channels. Please try again.
            </div>
          ) : filtered.length === 0 ? (
            <div className="py-8 text-center text-sm text-zinc-500">
              {search.trim()
                ? `No channels matching "${search}"`
                : 'No channels available to discover'}
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              {filtered.map((channel) => (
                <div
                  key={channel.id}
                  className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4 transition-colors hover:border-zinc-700"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <h3 className="truncate text-sm font-medium text-zinc-100">
                        {channel.name}
                      </h3>
                      {channel.description && (
                        <p className="mt-1 text-xs text-zinc-500 line-clamp-2">
                          {channel.description}
                        </p>
                      )}
                      <div className="mt-2 flex flex-wrap items-center gap-3 text-xs text-zinc-500">
                        <span className="inline-flex items-center gap-1">
                          <Users className="h-3 w-3" />
                          {channel.member_count}{' '}
                          {channel.member_count === 1 ? 'member' : 'members'}
                        </span>
                        <span>by {channel.owner_name}</span>
                      </div>
                    </div>
                    <div className="flex-shrink-0">
                      {channel.is_member ? (
                        <span className="inline-flex items-center rounded-md bg-emerald-900/40 px-2.5 py-1 text-xs font-medium text-emerald-400">
                          Joined
                        </span>
                      ) : (
                        <button
                          onClick={() =>
                            joinChannel.mutate(channel.id, {
                              onSuccess: () => {
                                // Keep the modal open so users can continue browsing
                              },
                            })
                          }
                          disabled={joinChannel.isPending}
                          className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {joinChannel.isPending ? (
                            <>
                              <Loader2 className="h-3 w-3 animate-spin" />
                              Joining...
                            </>
                          ) : (
                            'Join'
                          )}
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
