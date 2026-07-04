// Admin Console — audit log viewer (read-only).
// Lists admin actions with an action filter and prev/next pagination.
// Strictly read-only: no create/edit/delete affordances (plan T13 line 329).
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { ScrollText, Filter, AlertCircle, ChevronLeft, ChevronRight } from 'lucide-react'
import { listAuditLogs, type AuditLog } from '../../api/admin'

const PAGE_SIZE = 20

// Actions emitted by the backend (see T8 audit coverage). The empty
// option means "no filter" — all actions are returned.
const ACTION_FILTERS: { value: string; label: string }[] = [
  { value: '', label: 'All actions' },
  { value: 'admin.login', label: 'admin.login' },
  { value: 'user.update', label: 'user.update' },
  { value: 'user.disable', label: 'user.disable' },
  { value: 'user.reset_password', label: 'user.reset_password' },
  { value: 'user.delete', label: 'user.delete' },
  { value: 'invite.create', label: 'invite.create' },
  { value: 'invite.update', label: 'invite.update' },
  { value: 'invite.delete', label: 'invite.delete' },
]

function formatTimestamp(unixSeconds: number): string {
  return dayjs.unix(unixSeconds).format('YYYY-MM-DD HH:mm:ss')
}

function truncateDetails(details: string | null): string {
  if (details === null || details === '') return '—'
  return details
}

export default function AdminAuditLogsPage() {
  const [page, setPage] = useState(1)
  const [action, setAction] = useState('')

  const { data, isLoading, error, refetch } = useQuery<AuditLog[]>({
    queryKey: ['admin', 'audit-logs', page, action],
    queryFn: () =>
      listAuditLogs({
        page,
        limit: PAGE_SIZE,
        action: action === '' ? undefined : action,
      }),
  })

  const items = data ?? []
  const hasMore = items.length === PAGE_SIZE
  const isFirstPage = page === 1

  return (
    <div>
      <div className="mb-6 flex flex-wrap items-center justify-between gap-3">
        <h1 className="flex items-center gap-2 text-xl font-semibold text-white">
          <ScrollText className="h-5 w-5 text-zinc-400" />
          Audit Logs
        </h1>
        <div className="flex items-center gap-2">
          <label
            htmlFor="audit-action-filter"
            className="flex items-center gap-1.5 text-sm text-zinc-400"
          >
            <Filter className="h-4 w-4" />
            Action
          </label>
          <select
            id="audit-action-filter"
            value={action}
            onChange={(e) => {
              setAction(e.target.value)
              setPage(1)
            }}
            className="rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 focus:border-indigo-500/50 focus:outline-none focus:ring-2 focus:ring-indigo-500/30"
          >
            {ACTION_FILTERS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {isLoading && <TableSkeleton />}

      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-6">
          <div className="flex items-center gap-2 text-red-400">
            <AlertCircle className="h-5 w-5" />
            <p className="text-sm">
              {error instanceof Error
                ? error.message
                : 'Failed to load audit logs.'}
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

      {!isLoading && !error && items.length === 0 && (
        <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-12 text-center">
          <ScrollText className="mx-auto mb-3 h-8 w-8 text-zinc-600" />
          <p className="text-sm text-zinc-400">
            No audit logs{action ? ` for "${action}"` : ''} yet.
          </p>
        </div>
      )}

      {!isLoading && !error && items.length > 0 && (
        <>
          <div className="overflow-hidden rounded-lg border border-zinc-800 bg-zinc-900">
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead className="border-b border-zinc-800 bg-zinc-900 text-xs uppercase tracking-wide text-zinc-500">
                  <tr>
                    <th scope="col" className="px-4 py-3 font-medium">Action</th>
                    <th scope="col" className="px-4 py-3 font-medium">Target Type</th>
                    <th scope="col" className="px-4 py-3 font-medium">Target ID</th>
                    <th scope="col" className="px-4 py-3 font-medium">Details</th>
                    <th scope="col" className="px-4 py-3 font-medium">Performed At</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-zinc-800">
                  {items.map((row) => (
                    <AuditLogRow key={row.id} row={row} />
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div className="mt-4 flex items-center justify-between">
            <p className="text-xs text-zinc-500">
              Page {page} · {items.length} {items.length === 1 ? 'entry' : 'entries'}
            </p>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={isFirstPage}
                className="inline-flex items-center gap-1 rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors enabled:hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-40"
              >
                <ChevronLeft className="h-4 w-4" />
                Prev
              </button>
              <button
                type="button"
                onClick={() => setPage((p) => p + 1)}
                disabled={!hasMore}
                className="inline-flex items-center gap-1 rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors enabled:hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-40"
              >
                Next
                <ChevronRight className="h-4 w-4" />
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

function AuditLogRow({ row }: { row: AuditLog }) {
  const details = truncateDetails(row.details)
  const isJson = details !== '—' && details.startsWith('{')
  return (
    <tr className="align-top text-zinc-300">
      <td className="whitespace-nowrap px-4 py-3 font-mono text-xs text-zinc-100">
        {row.action}
      </td>
      <td className="px-4 py-3 text-zinc-400">
        {row.target_type ?? '—'}
      </td>
      <td className="px-4 py-3 font-mono text-xs text-zinc-400">
        {row.target_id ?? '—'}
      </td>
      <td className="max-w-md px-4 py-3 text-xs text-zinc-400">
        {isJson ? (
          <pre className="overflow-x-auto rounded bg-zinc-800/60 p-2 font-mono text-xs text-zinc-300">
            {prettyJson(details)}
          </pre>
        ) : (
          <span className="break-words">{details}</span>
        )}
      </td>
      <td className="whitespace-nowrap px-4 py-3 font-mono text-xs text-zinc-300 tabular-nums">
        <time dateTime={new Date(row.performed_at * 1000).toISOString()}>
          {formatTimestamp(row.performed_at)}
        </time>
      </td>
    </tr>
  )
}

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2)
  } catch {
    return raw
  }
}

function TableSkeleton() {
  return (
    <div className="overflow-hidden rounded-lg border border-zinc-800 bg-zinc-900">
      <div className="overflow-x-auto">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-zinc-800 bg-zinc-900 text-xs uppercase tracking-wide text-zinc-500">
            <tr>
              <th scope="col" className="px-4 py-3 font-medium">Action</th>
              <th scope="col" className="px-4 py-3 font-medium">Target Type</th>
              <th scope="col" className="px-4 py-3 font-medium">Target ID</th>
              <th scope="col" className="px-4 py-3 font-medium">Details</th>
              <th scope="col" className="px-4 py-3 font-medium">Performed At</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-800">
            {Array.from({ length: 6 }).map((_, i) => (
              <tr key={i} className="animate-pulse">
                <td className="px-4 py-3"><div className="h-4 w-24 rounded bg-zinc-800" /></td>
                <td className="px-4 py-3"><div className="h-4 w-16 rounded bg-zinc-800" /></td>
                <td className="px-4 py-3"><div className="h-4 w-20 rounded bg-zinc-800" /></td>
                <td className="px-4 py-3"><div className="h-4 w-48 rounded bg-zinc-800" /></td>
                <td className="px-4 py-3"><div className="h-4 w-32 rounded bg-zinc-800" /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
