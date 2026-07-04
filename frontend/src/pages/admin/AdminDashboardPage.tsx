// Admin Console — dashboard overview with stat cards.
// Uses React Query (same pattern as SearchPage, useChannels, etc.).
import { useQuery } from '@tanstack/react-query'
import {
  Users,
  Activity,
  Hash,
  MessageSquare,
  Ticket,
  TicketCheck,
  AlertCircle,
} from 'lucide-react'
import { getDashboard, type DashboardStats } from '../../api/admin'

interface StatCardConfig {
  key: keyof DashboardStats
  label: string
  icon: typeof Users
}

const STAT_CARDS: StatCardConfig[] = [
  { key: 'total_users', label: 'Total Users', icon: Users },
  { key: 'active_sessions_24h', label: 'Active Sessions (24h)', icon: Activity },
  { key: 'total_channels', label: 'Total Channels', icon: Hash },
  { key: 'total_messages', label: 'Total Messages', icon: MessageSquare },
  { key: 'total_invite_codes', label: 'Total Invite Codes', icon: Ticket },
  { key: 'active_invite_codes', label: 'Active Invite Codes', icon: TicketCheck },
]

function StatCard({ config, value }: { config: StatCardConfig; value: number }) {
  const Icon = config.icon
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-6">
      <div className="flex items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-zinc-800">
          <Icon className="h-5 w-5 text-zinc-400" />
        </div>
        <div>
          <p className="text-2xl font-semibold text-white tabular-nums">
            {value.toLocaleString()}
          </p>
          <p className="text-sm text-zinc-400">{config.label}</p>
        </div>
      </div>
    </div>
  )
}

function StatCardSkeleton() {
  return (
    <div className="animate-pulse rounded-lg border border-zinc-800 bg-zinc-900 p-6">
      <div className="flex items-center gap-3">
        <div className="h-10 w-10 rounded-lg bg-zinc-800" />
        <div className="space-y-2">
          <div className="h-6 w-20 rounded bg-zinc-800" />
          <div className="h-4 w-28 rounded bg-zinc-800" />
        </div>
      </div>
    </div>
  )
}

export default function AdminDashboardPage() {
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['admin', 'dashboard'],
    queryFn: getDashboard,
  })

  return (
    <div>
      <h1 className="mb-6 text-xl font-semibold text-white">Dashboard</h1>

      {isLoading && (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
          {STAT_CARDS.map((c) => (
            <StatCardSkeleton key={c.key} />
          ))}
        </div>
      )}

      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-6">
          <div className="flex items-center gap-2 text-red-400">
            <AlertCircle className="h-5 w-5" />
            <p className="text-sm">
              {error instanceof Error
                ? error.message
                : 'Failed to load dashboard.'}
            </p>
          </div>
          <button
            onClick={() => refetch()}
            className="mt-3 rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors hover:bg-zinc-800"
          >
            Retry
          </button>
        </div>
      )}

      {data && (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
          {STAT_CARDS.map((config) => (
            <StatCard
              key={config.key}
              config={config}
              value={data[config.key]}
            />
          ))}
        </div>
      )}
    </div>
  )
}
