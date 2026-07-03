import { useState, useEffect, useRef } from 'react'
import { useNavigate } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, Search, Loader2, Hash } from 'lucide-react'
import dayjs from 'dayjs'
import { apiClient, ApiClientError } from '../api/client'
import { useChannelStore } from '../stores/channelStore'
import { NoSearchResultsEmpty } from '../components/EmptyState'

interface SearchResultItem {
  message: { text?: string } & Record<string, unknown>
  snippet: string
  channel_id: string
  created_at: number
}

interface SearchResponse {
  results: SearchResultItem[]
}

const DEBOUNCE_MS = 300

export function SearchPage() {
  const navigate = useNavigate()
  const [input, setInput] = useState('')
  const [debounced, setDebounced] = useState('')
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    if (debounceTimer.current) clearTimeout(debounceTimer.current)
    const trimmed = input.trim()
    if (trimmed.length === 0) {
      setDebounced('')
      return
    }
    debounceTimer.current = setTimeout(() => setDebounced(trimmed), DEBOUNCE_MS)
    return () => {
      if (debounceTimer.current) clearTimeout(debounceTimer.current)
    }
  }, [input])

  const { data: results, isLoading, error } = useQuery<SearchResponse>({
    queryKey: ['search', debounced],
    queryFn: () =>
      apiClient<SearchResponse>(`/search?q=${encodeURIComponent(debounced)}`),
    enabled: debounced.length > 0,
  })

  const hasQuery = debounced.length > 0
  const list = results?.results ?? []

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      <header className="border-b border-zinc-800 px-6 py-3">
        <button
          onClick={() => navigate('/channels')}
          className="mb-2 flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to channels
        </button>
        <div className="flex items-center gap-2">
          <Search className="h-4 w-4 text-zinc-500" />
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Search messages across your channels..."
            autoFocus
            className="flex-1 bg-transparent text-sm text-zinc-100 placeholder-zinc-500 outline-none"
          />
          {isLoading && hasQuery && (
            <Loader2 className="h-4 w-4 animate-spin text-zinc-500" />
          )}
        </div>
      </header>

      <main className="flex-1 overflow-y-auto">
        {!hasQuery ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <Search className="mb-3 h-10 w-10 text-zinc-700" />
            <p className="text-sm text-zinc-500">Search for messages</p>
            <p className="mt-1 text-xs text-zinc-600">
              Results include messages from all channels you belong to
            </p>
          </div>
        ) : error ? (
          <SearchError error={error} />
        ) : isLoading ? (
          <div className="flex items-center justify-center py-16">
            <Loader2 className="h-6 w-6 animate-spin text-zinc-600" />
          </div>
        ) : list.length === 0 ? (
          <NoSearchResultsEmpty query={debounced} />
        ) : (
          <ul className="flex flex-col divide-y divide-zinc-900">
            {list.map((item, idx) => (
              <SearchResultRow key={`${item.channel_id}-${item.created_at}-${idx}`} item={item} />
            ))}
          </ul>
        )}
      </main>
    </div>
  )
}

function SearchResultRow({ item }: { item: SearchResultItem }) {
  const navigate = useNavigate()
  const channels = useChannelStore((s) => s.channels)
  const channel = channels.find((c) => c.id === item.channel_id)
  const channelName = channel?.name ?? item.channel_id.slice(0, 8)

  const openChannel = () => navigate(`/channels/${item.channel_id}`)

  return (
    <li
      onClick={openChannel}
      className="cursor-pointer px-6 py-3 hover:bg-zinc-900/60 transition-colors"
    >
      <div className="mb-1 flex items-center gap-2 text-xs text-zinc-500">
        <Hash className="h-3 w-3" />
        <span className="text-zinc-400">{channelName}</span>
        <span>·</span>
        <span>{dayjs.unix(item.created_at).format('MMM D, h:mm A')}</span>
      </div>
      <p
        className="text-sm text-zinc-200 leading-relaxed"
        // snippet comes from the server wrapped in <mark> tags for highlights
        dangerouslySetInnerHTML={{ __html: item.snippet }}
      />
    </li>
  )
}

function SearchError({ error }: { error: unknown }) {
  const message =
    error instanceof ApiClientError ? error.message : 'Search failed. Try again.'
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <p className="text-sm text-red-400">{message}</p>
    </div>
  )
}
