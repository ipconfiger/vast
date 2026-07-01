import { Hash, Search, MessageSquare, Plus } from 'lucide-react'

interface EmptyStateProps {
  icon?: React.ReactNode
  title: string
  description?: string
  action?: React.ReactNode
}

function EmptyStateBase({ icon, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
      {icon && (
        <div className="rounded-full bg-zinc-800/50 p-3 text-zinc-500">
          {icon}
        </div>
      )}
      <div>
        <h3 className="text-sm font-medium text-zinc-300">{title}</h3>
        {description && (
          <p className="mt-1 max-w-xs text-xs text-zinc-500">{description}</p>
        )}
      </div>
      {action && <div className="mt-2">{action}</div>}
    </div>
  )
}

export function NoChannelsEmpty() {
  return (
    <EmptyStateBase
      icon={<Hash className="h-5 w-5" />}
      title="No channels yet"
      description="Create a channel or join an existing one to start collaborating with your team."
      action={
        <div className="flex flex-col gap-2 text-xs text-zinc-500">
          <div className="flex items-center gap-2">
            <span className="flex h-5 w-5 items-center justify-center rounded bg-zinc-800 text-zinc-400">
              <Plus className="h-3 w-3" />
            </span>
            Click the + button above to create a channel
          </div>
          <div className="flex items-center gap-2">
            <span className="flex h-5 w-5 items-center justify-center rounded bg-zinc-800 text-zinc-400">
              <Search className="h-3 w-3" />
            </span>
            Browse public channels to find your team
          </div>
        </div>
      }
    />
  )
}

export function NoMessagesEmpty() {
  return (
    <EmptyStateBase
      icon={<MessageSquare className="h-5 w-5" />}
      title="No messages yet"
      description="This is the beginning of the conversation. Send a message to get started."
    />
  )
}

export function NoSearchResultsEmpty({ query }: { query?: string }) {
  return (
    <EmptyStateBase
      icon={<Search className="h-5 w-5" />}
      title={query ? `No results for "${query}"` : 'No results found'}
      description="Try adjusting your search terms or check a different channel."
    />
  )
}

export function SelectChannelPrompt() {
  return (
    <EmptyStateBase
      icon={<Hash className="h-5 w-5" />}
      title="Select a channel"
      description="Choose a channel from the sidebar to start messaging."
    />
  )
}
