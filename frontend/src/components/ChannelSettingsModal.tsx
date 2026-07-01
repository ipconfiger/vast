import { useState, useEffect } from 'react'
import {
  X,
  Settings,
  Loader2,
  Archive,
  ArchiveRestore,
} from 'lucide-react'
import { useChannel } from '../api/channels'
import { useUpdateChannel } from '../api/permissions'
import { useAuthStore } from '../stores/authStore'
import { MemberList } from './MemberList'

interface ChannelSettingsModalProps {
  channelId: string
  isOpen: boolean
  onClose: () => void
}

type Tab = 'general' | 'members' | 'danger'

export function ChannelSettingsModal({
  channelId,
  isOpen,
  onClose,
}: ChannelSettingsModalProps) {
  const { data: channel, isLoading } = useChannel(channelId)
  const updateChannel = useUpdateChannel()
  const user = useAuthStore((s) => s.user)
  const [activeTab, setActiveTab] = useState<Tab>('general')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [hasChanges, setHasChanges] = useState(false)

  const isOwner = channel?.created_by === user?.id

  useEffect(() => {
    if (channel) {
      setName(channel.name)
      setDescription(channel.description ?? '')
      setHasChanges(false)
    }
  }, [channel])

  useEffect(() => {
    if (channel) {
      const nameChanged = name !== channel.name
      const descChanged = description !== (channel.description ?? '')
      setHasChanges(nameChanged || descChanged)
    }
  }, [name, description, channel])

  const handleSave = () => {
    updateChannel.mutate(
      {
        channelId,
        data: {
          name: name.trim() || undefined,
          description: description.trim() || undefined,
        },
      },
      {
        onSuccess: () => setHasChanges(false),
      },
    )
  }

  const handleArchiveToggle = () => {
    const targetArchived = !(channel as { archived?: boolean }).archived
    updateChannel.mutate({
      channelId,
      data: { archived: targetArchived },
    })
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative w-full max-w-lg rounded-2xl border border-zinc-800 bg-zinc-950 shadow-2xl shadow-black/50">
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <div className="flex items-center gap-2">
            <Settings className="h-5 w-5 text-zinc-400" />
            <h2 className="text-lg font-semibold text-zinc-100">
              Channel Settings
            </h2>
          </div>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-zinc-500 hover:text-zinc-300 transition-colors"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {isLoading ? (
          <div className="flex items-center justify-center py-16">
            <Loader2 className="h-6 w-6 animate-spin text-zinc-600" />
          </div>
        ) : (
          <>
            <div className="flex border-b border-zinc-800">
              {([
                ['general', 'General'],
                ['members', 'Members'],
                ...(isOwner ? [['danger', 'Danger Zone']] as const : []),
              ] as const).map(([tab, label]) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab as Tab)}
                  className={`flex-1 px-4 py-2.5 text-sm font-medium transition-colors ${
                    activeTab === tab
                      ? 'border-b-2 border-indigo-500 text-indigo-400'
                      : 'text-zinc-500 hover:text-zinc-300'
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>

            <div className="px-6 py-4">
              {activeTab === 'general' && (
                <div className="space-y-4">
                  <div>
                    <label
                      htmlFor="channel-name"
                      className="block text-sm font-medium text-zinc-300 mb-1.5"
                    >
                      Channel Name
                    </label>
                    <input
                      id="channel-name"
                      type="text"
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                      disabled={!isOwner}
                      className="w-full rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500/50 transition-all disabled:opacity-60 disabled:cursor-not-allowed"
                    />
                  </div>
                  <div>
                    <label
                      htmlFor="channel-desc"
                      className="block text-sm font-medium text-zinc-300 mb-1.5"
                    >
                      Description
                    </label>
                    <textarea
                      id="channel-desc"
                      value={description}
                      onChange={(e) => setDescription(e.target.value)}
                      disabled={!isOwner}
                      rows={3}
                      className="w-full rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500/50 transition-all resize-none disabled:opacity-60 disabled:cursor-not-allowed"
                    />
                  </div>
                  {isOwner && hasChanges && (
                    <button
                      onClick={handleSave}
                      disabled={updateChannel.isPending}
                      className="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-500 disabled:opacity-50 flex items-center justify-center gap-2"
                    >
                      {updateChannel.isPending ? (
                        <>
                          <Loader2 className="h-4 w-4 animate-spin" />
                          Saving...
                        </>
                      ) : (
                        'Save Changes'
                      )}
                    </button>
                  )}
                  {isOwner && updateChannel.isSuccess && (
                    <p className="text-xs text-emerald-400 text-center">
                      Changes saved successfully
                    </p>
                  )}
                </div>
              )}

              {activeTab === 'members' && (
                <div className="max-h-64 overflow-y-auto">
                  <MemberList channelId={channelId} />
                </div>
              )}

              {activeTab === 'danger' && isOwner && (
                <div className="space-y-4">
                  <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4">
                    <h3 className="text-sm font-medium text-red-400">
                      {(channel as { archived?: boolean }).archived
                        ? 'Restore Channel'
                        : 'Archive Channel'}
                    </h3>
                    <p className="mt-1 text-xs text-zinc-500">
                      {(channel as { archived?: boolean }).archived
                        ? 'Restoring will make the channel active again.'
                        : 'Archiving will disable new messages. This action can be reversed.'}
                    </p>
                    <button
                      onClick={handleArchiveToggle}
                      disabled={updateChannel.isPending}
                      className="mt-3 flex items-center gap-2 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm text-red-400 transition-colors hover:bg-red-500/20 disabled:opacity-50"
                    >
                      {updateChannel.isPending ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (channel as { archived?: boolean }).archived ? (
                        <ArchiveRestore className="h-4 w-4" />
                      ) : (
                        <Archive className="h-4 w-4" />
                      )}
                      {(channel as { archived?: boolean }).archived
                        ? 'Restore Channel'
                        : 'Archive Channel'}
                    </button>
                  </div>
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}
