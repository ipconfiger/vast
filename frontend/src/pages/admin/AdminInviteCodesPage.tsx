// Admin Console — invite-code management page.
// Lists invite codes with create / toggle-active / reset-count / delete actions.
// Uses @tanstack/react-query for list + mutations and toastStore for feedback.
import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Plus,
  Ticket,
  Trash2,
  RotateCcw,
  Power,
  Loader2,
  AlertCircle,
  X,
} from 'lucide-react'
import dayjs from 'dayjs'
import {
  listInviteCodes,
  createInviteCode,
  updateInviteCode,
  deleteInviteCode,
  type InviteCode,
} from '../../api/admin'
import { toast } from '../../stores/toastStore'

const QUERY_KEY = ['admin', 'invite-codes'] as const

// Duck-typed 409 check: AdminApiClientError has .status, but instanceof is
// fragile across vi.mock boundaries (see T9 learnings). Field check is robust.
function isConflictError(e: unknown): boolean {
  return e instanceof Error && (e as { status?: number }).status === 409
}

function UsageBadge({ code }: { code: InviteCode }) {
  const ratio = code.max_uses === 0 ? 1 : code.use_count / code.max_uses
  const text = `${code.use_count}/${code.max_uses}`
  if (ratio >= 1) {
    return (
      <span className="inline-flex rounded-full bg-red-500/20 px-2 py-0.5 text-xs text-red-300 tabular-nums">
        {text}
      </span>
    )
  }
  if (ratio >= 0.8) {
    return (
      <span className="inline-flex rounded-full bg-amber-500/20 px-2 py-0.5 text-xs text-amber-300 tabular-nums">
        {text}
      </span>
    )
  }
  return (
    <span className="inline-flex rounded-full bg-zinc-700 px-2 py-0.5 text-xs text-zinc-300 tabular-nums">
      {text}
    </span>
  )
}

function StatusBadge({ isActive }: { isActive: boolean }) {
  return isActive ? (
    <span className="inline-flex items-center rounded-full bg-emerald-500/20 px-2 py-0.5 text-xs text-emerald-300">
      Active
    </span>
  ) : (
    <span className="inline-flex items-center rounded-full bg-zinc-700 px-2 py-0.5 text-xs text-zinc-400">
      Inactive
    </span>
  )
}

export default function AdminInviteCodesPage() {
  const queryClient = useQueryClient()
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: QUERY_KEY,
    queryFn: () => listInviteCodes({ page: 1, limit: 100 }),
  })

  const [modalOpen, setModalOpen] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null)

  const invalidateList = () =>
    queryClient.invalidateQueries({ queryKey: QUERY_KEY })

  const createMutation = useMutation({
    mutationFn: createInviteCode,
    onSuccess: (created) => {
      toast.success(`Invite code "${created.code}" created`)
      setModalOpen(false)
      invalidateList()
    },
    onError: (e) => {
      // Inline 409 handled by the modal; surface others as toast.
      if (!isConflictError(e)) {
        toast.error(e instanceof Error ? e.message : 'Failed to create invite code')
      }
    },
  })

  const toggleMutation = useMutation({
    mutationFn: (vars: { code: string; is_active: boolean }) =>
      updateInviteCode(vars.code, { is_active: vars.is_active }),
    onSuccess: (updated) => {
      toast.success(
        `Invite code "${updated.code}" ${updated.is_active ? 'activated' : 'deactivated'}`,
      )
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to update invite code')
    },
  })

  const resetMutation = useMutation({
    mutationFn: (code: string) => updateInviteCode(code, { reset_use_count: true }),
    onSuccess: (updated) => {
      toast.success(`Use count for "${updated.code}" reset to 0`)
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to reset use count')
    },
  })

  const deleteMutation = useMutation({
    mutationFn: deleteInviteCode,
    onSuccess: () => {
      toast.success(`Invite code "${deleteTarget}" deleted`)
      setDeleteTarget(null)
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to delete invite code')
    },
  })

  const codes = data ?? []

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-xl font-semibold text-white">Invite Codes</h1>
        <button
          type="button"
          onClick={() => setModalOpen(true)}
          className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-500"
        >
          <Plus className="h-4 w-4" />
          New Invite Code
        </button>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center py-16">
          <Loader2 className="h-6 w-6 animate-spin text-zinc-600" />
        </div>
      )}

      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-6">
          <div className="flex items-center gap-2 text-red-400">
            <AlertCircle className="h-5 w-5" />
            <p className="text-sm">
              {error instanceof Error
                ? error.message
                : 'Failed to load invite codes.'}
            </p>
          </div>
          <button
            type="button"
            onClick={() => refetch()}
            className="mt-3 rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition-colors hover:bg-zinc-800"
          >
            Retry
          </button>
        </div>
      )}

      {!isLoading && !error && codes.length === 0 && (
        <div className="flex flex-col items-center justify-center py-16 text-center">
          <Ticket className="mb-3 h-10 w-10 text-zinc-700" />
          <p className="text-sm text-zinc-500">No invite codes yet</p>
          <p className="mt-1 text-xs text-zinc-600">
            Create one to allow new user registrations
          </p>
        </div>
      )}

      {!isLoading && !error && codes.length > 0 && (
        <div className="overflow-hidden rounded-lg border border-zinc-800">
          <table className="w-full text-sm">
            <thead className="bg-zinc-900 text-left text-xs uppercase tracking-wider text-zinc-500">
              <tr>
                <th className="px-4 py-3 font-medium">Code</th>
                <th className="px-4 py-3 font-medium">Usage</th>
                <th className="px-4 py-3 font-medium">Status</th>
                <th className="px-4 py-3 font-medium">Created</th>
                <th className="px-4 py-3 text-right font-medium">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-800 bg-zinc-900/50">
              {codes.map((code) => (
                <tr key={code.code} className="hover:bg-zinc-900">
                  <td className="px-4 py-3 font-mono text-zinc-100">{code.code}</td>
                  <td className="px-4 py-3">
                    <UsageBadge code={code} />
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge isActive={code.is_active} />
                  </td>
                  <td className="px-4 py-3 text-zinc-400">
                    {dayjs.unix(code.created_at).format('MMM D, YYYY')}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-2">
                      <button
                        type="button"
                        onClick={() =>
                          toggleMutation.mutate({
                            code: code.code,
                            is_active: !code.is_active,
                          })
                        }
                        disabled={
                          toggleMutation.isPending &&
                          toggleMutation.variables?.code === code.code
                        }
                        title={code.is_active ? 'Deactivate' : 'Activate'}
                        aria-label={`Toggle ${code.code}`}
                        className="rounded-md border border-zinc-700 p-1.5 text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-200 disabled:opacity-50"
                      >
                        <Power className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => resetMutation.mutate(code.code)}
                        disabled={
                          resetMutation.isPending &&
                          resetMutation.variables === code.code
                        }
                        title="Reset use count"
                        aria-label={`Reset ${code.code}`}
                        className="rounded-md border border-zinc-700 p-1.5 text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-200 disabled:opacity-50"
                      >
                        <RotateCcw className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => setDeleteTarget(code.code)}
                        title="Delete"
                        aria-label={`Delete ${code.code}`}
                        className="rounded-md border border-zinc-700 p-1.5 text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-red-300"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {modalOpen && (
        <CreateInviteCodeModal
          isPending={createMutation.isPending}
          submitError={createMutation.error}
          onClose={() => setModalOpen(false)}
          onSubmit={(payload) => createMutation.mutate(payload)}
        />
      )}

      {deleteTarget && (
        <ConfirmDeleteDialog
          code={deleteTarget}
          isPending={deleteMutation.isPending}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => deleteMutation.mutate(deleteTarget)}
        />
      )}
    </div>
  )
}

// --- Create modal ---------------------------------------------------------

interface CreatePayload {
  code: string
  max_uses: number
  is_active: boolean
}

interface CreateModalProps {
  isPending: boolean
  submitError: unknown
  onClose: () => void
  onSubmit: (payload: CreatePayload) => void
}

function CreateInviteCodeModal({
  isPending,
  submitError,
  onClose,
  onSubmit,
}: CreateModalProps) {
  const [code, setCode] = useState('')
  const [maxUses, setMaxUses] = useState('100')
  const [isActive, setIsActive] = useState(true)

  const trimmedCode = code.trim()
  const canSubmit =
    trimmedCode.length >= 1 && trimmedCode.length <= 64 && !isPending

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!canSubmit) return
    const parsed = Number(maxUses)
    onSubmit({
      code: trimmedCode,
      max_uses: Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : 100,
      is_active: isActive,
    })
  }

  const conflict = isConflictError(submitError)

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 shadow-2xl shadow-black/50">
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <h2 className="text-lg font-semibold text-zinc-100">New Invite Code</h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-zinc-500 hover:text-zinc-300 transition-colors"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4 px-6 py-4">
          <div>
            <label
              htmlFor="ic-code"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              Code
            </label>
            <input
              id="ic-code"
              type="text"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              required
              maxLength={64}
              placeholder="e.g. WELCOME2026"
              className="w-full rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 transition-all focus:border-indigo-500/50 focus:outline-none focus:ring-2 focus:ring-indigo-500/50"
            />
          </div>
          <div>
            <label
              htmlFor="ic-max-uses"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              Max Uses
            </label>
            <input
              id="ic-max-uses"
              type="number"
              value={maxUses}
              onChange={(e) => setMaxUses(e.target.value)}
              min={0}
              className="w-full rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 transition-all focus:border-indigo-500/50 focus:outline-none focus:ring-2 focus:ring-indigo-500/50"
            />
          </div>
          <label className="flex items-center gap-2 text-sm text-zinc-300">
            <input
              id="ic-active"
              type="checkbox"
              checked={isActive}
              onChange={(e) => setIsActive(e.target.checked)}
              className="h-4 w-4 rounded border-zinc-700 bg-zinc-800"
            />
            Active
          </label>
          {conflict && (
            <p className="text-sm text-red-400" role="alert">
              An invite code with this value already exists.
            </p>
          )}
          {submitError instanceof Error && !conflict && (
            <p className="text-sm text-red-400" role="alert">
              {submitError.message}
            </p>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg border border-zinc-700 px-4 py-2 text-sm font-medium text-zinc-300 transition-colors hover:bg-zinc-800"
            >
              Cancel
            </button>
            <button
              id="ic-submit"
              type="submit"
              disabled={!canSubmit}
              className="flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isPending ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Creating...
                </>
              ) : (
                'Create'
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// --- Confirm delete dialog ------------------------------------------------

interface ConfirmDeleteProps {
  code: string
  isPending: boolean
  onCancel: () => void
  onConfirm: () => void
}

function ConfirmDeleteDialog({
  code,
  isPending,
  onCancel,
  onConfirm,
}: ConfirmDeleteProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onCancel}
      />
      <div className="relative w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 shadow-2xl shadow-black/50">
        <div className="border-b border-zinc-800 px-6 py-4">
          <h2 className="text-lg font-semibold text-zinc-100">Delete Invite Code</h2>
        </div>
        <div className="px-6 py-4">
          <p className="text-sm text-zinc-300">
            Are you sure you want to delete invite code{' '}
            <span className="font-mono text-white">{code}</span>? This action cannot
            be undone.
          </p>
        </div>
        <div className="flex justify-end gap-2 border-t border-zinc-800 px-6 py-4">
          <button
            type="button"
            onClick={onCancel}
            disabled={isPending}
            className="rounded-lg border border-zinc-700 px-4 py-2 text-sm font-medium text-zinc-300 transition-colors hover:bg-zinc-800 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            id="confirm-delete"
            onClick={onConfirm}
            disabled={isPending}
            className="flex items-center gap-2 rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Deleting...
              </>
            ) : (
              <>
                <Trash2 className="h-4 w-4" />
                Delete
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  )
}
