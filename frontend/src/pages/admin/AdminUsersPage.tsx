// Admin Console — user management page.
// Search + paginated table + disable/enable, reset-password, delete actions.
// The AdminUser type does not expose a disabled flag (the backend uses
// token_epoch revocation server-side), so disabled state is tracked locally
// in this session via disabledMap and applied to the button label/badge.
import { useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Search, AlertCircle, Ban, KeyRound, Trash2, X } from 'lucide-react'
import dayjs from 'dayjs'
import {
  listUsers,
  updateUser,
  resetUserPassword,
  deleteUser,
  AdminApiClientError,
  type AdminUser,
} from '../../api/admin'
import { toast } from '../../stores/toastStore'

const PAGE_SIZE = 10
const DEBOUNCE_MS = 300

export default function AdminUsersPage() {
  const queryClient = useQueryClient()
  const [searchInput, setSearchInput] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')
  const [page, setPage] = useState(1)
  const [disabledMap, setDisabledMap] = useState<Record<string, boolean>>({})

  // Reset-password modal state
  const [resetTarget, setResetTarget] = useState<AdminUser | null>(null)
  const [newPassword, setNewPassword] = useState('')
  const [resetError, setResetError] = useState<string | null>(null)

  // Delete confirm dialog state
  const [deleteTarget, setDeleteTarget] = useState<AdminUser | null>(null)

  // Debounce search input; reset to page 1 whenever the query changes.
  useEffect(() => {
    const t = setTimeout(() => {
      setDebouncedQuery(searchInput.trim())
      setPage(1)
    }, DEBOUNCE_MS)
    return () => clearTimeout(t)
  }, [searchInput])

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['admin', 'users', debouncedQuery, page],
    queryFn: () =>
      listUsers({
        q: debouncedQuery || undefined,
        page,
        limit: PAGE_SIZE,
      }),
  })

  const disableMutation = useMutation({
    mutationFn: (vars: { id: string; disabled: boolean }) =>
      updateUser(vars.id, { disabled: vars.disabled }),
    onSuccess: (_data, vars) => {
      setDisabledMap((prev) => ({ ...prev, [vars.id]: vars.disabled }))
      queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
      toast.success(
        vars.disabled ? 'User disabled (tokens revoked)' : 'User enabled',
      )
    },
    onError: (err: unknown) => {
      toast.error(err instanceof Error ? err.message : 'Failed to update user')
    },
  })

  const resetPasswordMutation = useMutation({
    mutationFn: (vars: { id: string; password: string }) =>
      resetUserPassword(vars.id, { new_password: vars.password }),
    onSuccess: () => {
      toast.success('Password reset successfully')
      closeResetModal()
      queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
    },
    onError: (err: unknown) => {
      // 422 carries the backend validation message; surface it inline.
      if (err instanceof AdminApiClientError && err.status === 422) {
        setResetError(err.message)
      } else {
        toast.error(
          err instanceof Error ? err.message : 'Failed to reset password',
        )
      }
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteUser(id),
    onSuccess: () => {
      toast.success('User deleted')
      setDeleteTarget(null)
      queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
    },
    onError: (err: unknown) => {
      toast.error(err instanceof Error ? err.message : 'Failed to delete user')
      setDeleteTarget(null)
    },
  })

  function closeResetModal() {
    setResetTarget(null)
    setNewPassword('')
    setResetError(null)
  }

  function handleResetSubmit() {
    if (!resetTarget) return
    setResetError(null)
    resetPasswordMutation.mutate({
      id: resetTarget.id,
      password: newPassword,
    })
  }

  const users = data ?? []
  const isLastPage = users.length < PAGE_SIZE

  return (
    <div>
      <h1 className="mb-6 text-xl font-semibold text-white">Users</h1>

      {/* Search bar */}
      <div className="mb-4 flex items-center gap-2 rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2">
        <Search className="h-4 w-4 text-zinc-500" />
        <input
          type="text"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder="Search by username..."
          className="flex-1 bg-transparent text-sm text-zinc-100 placeholder-zinc-500 outline-none"
        />
        {searchInput && (
          <button
            onClick={() => setSearchInput('')}
            className="text-zinc-500 hover:text-zinc-300"
            aria-label="Clear search"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      {/* Loading */}
      {isLoading && <UsersTableSkeleton />}

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-6">
          <div className="flex items-center gap-2 text-red-400">
            <AlertCircle className="h-5 w-5" />
            <p className="text-sm">
              {error instanceof Error
                ? error.message
                : 'Failed to load users.'}
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

      {/* Table */}
      {!isLoading && !error && (
        <>
          <div className="overflow-hidden rounded-lg border border-zinc-800">
            <table className="w-full text-left text-sm">
              <thead className="border-b border-zinc-800 bg-zinc-900 text-zinc-400">
                <tr>
                  <th className="px-4 py-3 font-medium">Username</th>
                  <th className="px-4 py-3 font-medium">Display Name</th>
                  <th className="px-4 py-3 font-medium">Created</th>
                  <th className="px-4 py-3 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800">
                {users.map((u) => {
                  const isDisabled = disabledMap[u.id] ?? false
                  return (
                    <tr key={u.id} className="bg-zinc-950/40">
                      <td className="px-4 py-3 text-zinc-100">
                        <div className="flex items-center gap-2">
                          <span>{u.username}</span>
                          {isDisabled && (
                            <span className="rounded border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-xs text-amber-400">
                              Disabled
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="px-4 py-3 text-zinc-300">
                        {u.display_name || (
                          <span className="text-zinc-600">—</span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-zinc-400">
                        {dayjs.unix(u.created_at).format('YYYY-MM-DD HH:mm')}
                      </td>
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-2">
                          <button
                            onClick={() =>
                              disableMutation.mutate({
                                id: u.id,
                                disabled: !isDisabled,
                              })
                            }
                            disabled={disableMutation.isPending}
                            className="inline-flex items-center gap-1 rounded-md border border-zinc-700 px-2 py-1 text-xs text-zinc-300 transition-colors hover:bg-zinc-800 disabled:opacity-50"
                          >
                            <Ban className="h-3 w-3" />
                            <span>{isDisabled ? 'Enable' : 'Disable'}</span>
                          </button>
                          <button
                            onClick={() => {
                              setResetTarget(u)
                              setNewPassword('')
                              setResetError(null)
                            }}
                            className="inline-flex items-center gap-1 rounded-md border border-zinc-700 px-2 py-1 text-xs text-zinc-300 transition-colors hover:bg-zinc-800"
                          >
                            <KeyRound className="h-3 w-3" />
                            <span>Reset Password</span>
                          </button>
                          <button
                            onClick={() => setDeleteTarget(u)}
                            className="inline-flex items-center gap-1 rounded-md border border-red-500/30 px-2 py-1 text-xs text-red-400 transition-colors hover:bg-red-500/10"
                          >
                            <Trash2 className="h-3 w-3" />
                            <span>Delete</span>
                          </button>
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>

          {/* Empty state */}
          {users.length === 0 && (
            <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-8 text-center">
              <p className="text-sm text-zinc-500">
                {debouncedQuery
                  ? `No users matching "${debouncedQuery}".`
                  : 'No users found.'}
              </p>
            </div>
          )}

          {/* Pagination */}
          {users.length > 0 && (
            <div className="mt-4 flex items-center justify-between">
              <button
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page === 1}
                className="rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-30"
              >
                Previous
              </button>
              <span className="text-sm text-zinc-400">Page {page}</span>
              <button
                onClick={() => setPage((p) => p + 1)}
                disabled={isLastPage}
                className="rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-30"
              >
                Next
              </button>
            </div>
          )}
        </>
      )}

      {/* Reset Password Modal */}
      {resetTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
          role="dialog"
          aria-modal="true"
        >
          <div className="w-96 rounded-lg border border-zinc-800 bg-zinc-900 p-6">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="text-base font-semibold text-white">
                Reset Password
              </h2>
              <button
                onClick={closeResetModal}
                className="text-zinc-500 hover:text-zinc-300"
                aria-label="Close"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <p className="mb-3 text-sm text-zinc-400">
              Set a new password for{' '}
              <span className="font-medium text-zinc-200">
                {resetTarget.username}
              </span>
              . Must be at least 8 characters with a letter and a digit.
            </p>
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              placeholder="New password"
              autoFocus
              className="w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 outline-none focus:border-indigo-500"
            />
            {resetError && (
              <p className="mt-2 text-sm text-red-400">{resetError}</p>
            )}
            <div className="mt-4 flex justify-end gap-2">
              <button
                onClick={closeResetModal}
                className="rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors hover:bg-zinc-800"
              >
                Cancel
              </button>
              <button
                onClick={handleResetSubmit}
                disabled={
                  resetPasswordMutation.isPending || newPassword.length === 0
                }
                className="rounded-md bg-indigo-600 px-3 py-1.5 text-sm text-white transition-colors hover:bg-indigo-500 disabled:opacity-50"
              >
                {resetPasswordMutation.isPending ? 'Resetting...' : 'Reset'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete Confirm Dialog */}
      {deleteTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
          role="dialog"
          aria-modal="true"
        >
          <div className="w-96 rounded-lg border border-zinc-800 bg-zinc-900 p-6">
            <h2 className="mb-3 text-base font-semibold text-white">
              Delete User
            </h2>
            <p className="mb-4 text-sm text-zinc-300">
              Are you sure you want to delete{' '}
              <span className="font-medium text-white">
                {deleteTarget.username}
              </span>
              ? This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setDeleteTarget(null)}
                className="rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors hover:bg-zinc-800"
              >
                Cancel
              </button>
              <button
                onClick={() => deleteMutation.mutate(deleteTarget.id)}
                disabled={deleteMutation.isPending}
                className="rounded-md bg-red-600 px-3 py-1.5 text-sm text-white transition-colors hover:bg-red-500 disabled:opacity-50"
              >
                {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function UsersTableSkeleton() {
  return (
    <div className="overflow-hidden rounded-lg border border-zinc-800">
      <div className="border-b border-zinc-800 bg-zinc-900 px-4 py-3">
        <div className="h-4 w-48 animate-pulse rounded bg-zinc-800" />
      </div>
      <div className="divide-y divide-zinc-800">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="flex items-center gap-4 px-4 py-3">
            <div className="h-4 w-24 animate-pulse rounded bg-zinc-800" />
            <div className="h-4 w-32 animate-pulse rounded bg-zinc-800" />
            <div className="h-4 w-28 animate-pulse rounded bg-zinc-800" />
            <div className="h-4 w-20 animate-pulse rounded bg-zinc-800" />
          </div>
        ))}
      </div>
    </div>
  )
}
