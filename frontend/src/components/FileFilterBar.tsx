import { useState, useEffect, useRef } from 'react'
import { Search, Grid3x3, List, Filter, X } from 'lucide-react'
import { useChannels } from '../api/channels'
import { useDms } from '../api/dm'
import type { FileFilters } from '../types'

const DEBOUNCE_MS = 300

const TYPE_OPTIONS = [
  { label: 'All types', value: '' },
  { label: 'Images', value: 'image/' },
  { label: 'Documents', value: 'application/pdf' },
  { label: 'Video', value: 'video/' },
  { label: 'Audio', value: 'audio/' },
  { label: 'Archives', value: 'application/zip' },
] as const

const SORT_OPTIONS = [
  { label: 'Created', value: 'created_at' as const },
  { label: 'Size', value: 'size' as const },
  { label: 'Name', value: 'name' as const },
]

interface ChannelOption {
  id: string
  name: string
  isArchived?: boolean
}

interface FileFilterBarProps {
  filters: FileFilters
  onFiltersChange: (f: FileFilters) => void
  viewMode: 'grid' | 'list'
  onViewModeChange: (m: 'grid' | 'list') => void
}

export function FileFilterBar({
  filters,
  onFiltersChange,
  viewMode,
  onViewModeChange,
}: FileFilterBarProps) {
  const { data: channelsData } = useChannels()
  const { data: dms } = useDms()
  const [searchInput, setSearchInput] = useState(filters.search ?? '')
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const filtersRef = useRef(filters)
  filtersRef.current = filters
  const onFiltersChangeRef = useRef(onFiltersChange)
  onFiltersChangeRef.current = onFiltersChange

  const [sizeMinKB, setSizeMinKB] = useState(
    filters.size_min ? String(filters.size_min / 1024) : ''
  )
  const [sizeMaxKB, setSizeMaxKB] = useState(
    filters.size_max ? String(filters.size_max / 1024) : ''
  )
  const [dateAfter, setDateAfter] = useState(
    filters.created_after
      ? dayjs_unix_to_date(filters.created_after)
      : ''
  )
  const [dateBefore, setDateBefore] = useState(
    filters.created_before
      ? dayjs_unix_to_date(filters.created_before)
      : ''
  )

  // Build merged channel list
  const channelOptions: ChannelOption[] = [
    { id: '', name: 'All channels' },
  ]

  if (channelsData) {
    for (const ch of channelsData) {
      channelOptions.push({
        id: ch.id,
        name: ch.name,
        isArchived: !!(ch as unknown as Record<string, unknown>).is_archived,
      })
    }
  }

  if (dms) {
    for (const dm of dms) {
      channelOptions.push({
        id: dm.id,
        name: dm.name,
        isArchived: dm.is_archived,
      })
    }
  }

  // Debounced search — uses refs so debounce always merges into latest filters
  useEffect(() => {
    if (debounceTimer.current) clearTimeout(debounceTimer.current)
    const trimmed = searchInput.trim()
    debounceTimer.current = setTimeout(() => {
      onFiltersChangeRef.current({
        ...filtersRef.current,
        search: trimmed || undefined,
        cursor: undefined,
      })
    }, DEBOUNCE_MS)
    return () => {
      if (debounceTimer.current) clearTimeout(debounceTimer.current)
    }
  }, [searchInput])

  const updateFilter = (patch: Partial<FileFilters>) => {
    // Reset cursor when any filter changes
    onFiltersChange({ ...filters, ...patch, cursor: undefined })
  }

  const today = new Date().toISOString().split('T')[0]

  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-zinc-800 bg-zinc-950 px-4 py-2.5">
      {/* Channel dropdown */}
      <div className="flex items-center gap-1.5">
        <Filter className="h-3.5 w-3.5 text-zinc-500" />
        <select
          value={filters.channel_id ?? ''}
          onChange={(e) =>
            updateFilter({ channel_id: e.target.value || undefined })
          }
          className="min-w-0 max-w-[180px] rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500"
        >
          {channelOptions.map((ch) => (
            <option key={ch.id} value={ch.id}>
              {ch.name}
              {ch.isArchived ? ' (Archived)' : ''}
            </option>
          ))}
        </select>
      </div>

      {/* Type dropdown */}
      <select
        value={filters.mime_prefix ?? ''}
        onChange={(e) =>
          updateFilter({ mime_prefix: e.target.value || undefined, mime_type: undefined })
        }
        className="rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500"
      >
        {TYPE_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>

      {/* Size range */}
      <div className="flex items-center gap-1 text-xs text-zinc-400">
        <input
          type="number"
          min="0"
          placeholder="Min"
          value={sizeMinKB}
          onChange={(e) => {
            setSizeMinKB(e.target.value)
            const val = Number(e.target.value)
            updateFilter({
              size_min: val > 0 ? val * 1024 : undefined,
            })
          }}
          className="w-16 rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
        <span>-</span>
        <input
          type="number"
          min="0"
          placeholder="Max"
          value={sizeMaxKB}
          onChange={(e) => {
            setSizeMaxKB(e.target.value)
            const val = Number(e.target.value)
            updateFilter({
              size_max: val > 0 ? val * 1024 : undefined,
            })
          }}
          className="w-16 rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
        <span>KB</span>
      </div>

      {/* Date range */}
      <div className="flex items-center gap-1">
        <input
          type="date"
          value={dateAfter}
          max={dateBefore || today}
          onChange={(e) => {
            setDateAfter(e.target.value)
            const ts = e.target.value
              ? Math.floor(new Date(e.target.value).getTime() / 1000)
              : undefined
            updateFilter({ created_after: ts })
          }}
          className="w-32 rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500"
        />
        <span className="text-xs text-zinc-500">-</span>
        <input
          type="date"
          value={dateBefore}
          min={dateAfter || undefined}
          max={today}
          onChange={(e) => {
            setDateBefore(e.target.value)
            const ts = e.target.value
              ? Math.floor(new Date(e.target.value + 'T23:59:59').getTime() / 1000)
              : undefined
            updateFilter({ created_before: ts })
          }}
          className="w-32 rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500"
        />
      </div>

      {/* Search input */}
      <div className="relative flex-1 min-w-[140px]">
        <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-500" />
        <input
          type="text"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder="Search files..."
          className="w-full rounded-md border border-zinc-700 bg-zinc-900 py-1.5 pl-7 pr-2 text-xs text-zinc-200 outline-none placeholder:text-zinc-500 focus:border-zinc-500"
        />
        {searchInput && (
          <button
            onClick={() => {
              setSearchInput('')
              updateFilter({ search: undefined })
            }}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-zinc-300"
            aria-label="Clear search"
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>

      {/* Sort */}
      <select
        value={filters.sort_by ?? 'created_at'}
        onChange={(e) =>
          updateFilter({
            sort_by: e.target.value as FileFilters['sort_by'],
            cursor: undefined,
          })
        }
        className="rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 outline-none focus:border-zinc-500"
      >
        {SORT_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      <select
        value={filters.sort_order ?? 'desc'}
        onChange={(e) =>
          updateFilter({
            sort_order: e.target.value as 'asc' | 'desc',
            cursor: undefined,
          })
        }
        className="rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-200 uppercase outline-none focus:border-zinc-500"
      >
        <option value="desc">Desc</option>
        <option value="asc">Asc</option>
      </select>

      {/* View toggle */}
      <div className="ml-auto flex items-center gap-0.5 rounded-md border border-zinc-700 bg-zinc-900 p-0.5">
        <button
          onClick={() => onViewModeChange('grid')}
          className={`rounded p-1 transition-colors ${
            viewMode === 'grid'
              ? 'bg-zinc-600 text-zinc-100'
              : 'text-zinc-500 hover:text-zinc-300'
          }`}
          aria-label="Grid view"
        >
          <Grid3x3 className="h-3.5 w-3.5" />
        </button>
        <button
          onClick={() => onViewModeChange('list')}
          className={`rounded p-1 transition-colors ${
            viewMode === 'list'
              ? 'bg-zinc-600 text-zinc-100'
              : 'text-zinc-500 hover:text-zinc-300'
          }`}
          aria-label="List view"
        >
          <List className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  )
}

/** Convert a unix timestamp (seconds) to YYYY-MM-DD for a date input. */
function dayjs_unix_to_date(ts: number): string {
  return new Date(ts * 1000).toISOString().split('T')[0]
}
