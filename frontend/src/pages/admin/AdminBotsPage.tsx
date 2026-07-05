import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Plus,
  Bot as BotIcon,
  Trash2,
  Power,
  Pencil,
  Loader2,
  AlertCircle,
  X,
  Zap,
} from 'lucide-react'
import dayjs from 'dayjs'
import {
  listBots,
  createBot,
  updateBot,
  deleteBot,
  testBot,
  type Bot,
} from '../../api/admin'
import { toast } from '../../stores/toastStore'

const QUERY_KEY = ['admin', 'bots'] as const

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

export default function AdminBotsPage() {
  const queryClient = useQueryClient()
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: QUERY_KEY,
    queryFn: listBots,
  })

  const [createOpen, setCreateOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<Bot | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Bot | null>(null)

  const bots = data ?? []

  const invalidateList = () =>
    queryClient.invalidateQueries({ queryKey: QUERY_KEY })

  const createMutation = useMutation({
    mutationFn: createBot,
    onSuccess: (created) => {
      toast.success(`Bot "${created.name}" created`)
      setCreateOpen(false)
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to create bot')
    },
  })

  const updateMutation = useMutation({
    mutationFn: (vars: { id: string; body: Parameters<typeof updateBot>[1] }) =>
      updateBot(vars.id, vars.body),
    onSuccess: (updated) => {
      toast.success(`Bot "${updated.name}" updated`)
      setEditTarget(null)
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to update bot')
    },
  })

  const toggleMutation = useMutation({
    mutationFn: (vars: { bot: Bot; is_active: boolean }) =>
      updateBot(vars.bot.id, { is_active: vars.is_active }),
    onSuccess: (updated) => {
      toast.success(
        `Bot "${updated.name}" ${updated.is_active ? 'activated' : 'deactivated'}`,
      )
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to update bot')
    },
  })

  const deleteMutation = useMutation({
    mutationFn: deleteBot,
    onSuccess: () => {
      toast.success(`Bot "${deleteTarget?.name}" deleted`)
      setDeleteTarget(null)
      invalidateList()
    },
    onError: (e) => {
      toast.error(e instanceof Error ? e.message : 'Failed to delete bot')
    },
  })

  const testMutation = useMutation({
    mutationFn: (id: string) => testBot(id),
    onSuccess: (result, id) => {
      const botName = bots.find((b) => b.id === id)?.name ?? 'Bot'
      if (result.ok) {
        const preview = (result.response ?? '').slice(0, 100)
        toast.success(
          preview
            ? `✅ 连接成功 — ${botName}: ${preview}`
            : `✅ 连接成功 — ${botName}`,
        )
      } else {
        toast.error(`❌ 连接失败 — ${botName}: ${result.error ?? 'unknown error'}`)
      }
    },
    onError: (e, id) => {
      const botName = bots.find((b) => b.id === id)?.name ?? 'Bot'
      toast.error(
        e instanceof Error
          ? `❌ 连接失败 — ${botName}: ${e.message}`
          : `❌ 连接失败 — ${botName}`,
      )
    },
  })

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-xl font-semibold text-white">Bots</h1>
        <button
          type="button"
          onClick={() => setCreateOpen(true)}
          className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-500"
        >
          <Plus className="h-4 w-4" />
          New Bot
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
              {error instanceof Error ? error.message : 'Failed to load bots.'}
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

      {!isLoading && !error && bots.length === 0 && (
        <div className="flex flex-col items-center justify-center py-16 text-center">
          <BotIcon className="mb-3 h-10 w-10 text-zinc-700" />
          <p className="text-sm text-zinc-500">No bots yet</p>
          <p className="mt-1 text-xs text-zinc-600">
            Create one to enable AI assistants in channels
          </p>
        </div>
      )}

      {!isLoading && !error && bots.length > 0 && (
        <div className="overflow-hidden rounded-lg border border-zinc-800">
          <table className="w-full text-sm">
            <thead className="bg-zinc-900 text-left text-xs uppercase tracking-wider text-zinc-500">
              <tr>
                <th className="px-4 py-3 font-medium">Name</th>
                <th className="px-4 py-3 font-medium">Display Name</th>
                <th className="px-4 py-3 font-medium">API URL</th>
                <th className="px-4 py-3 font-medium">Status</th>
                <th className="px-4 py-3 font-medium">Created</th>
                <th className="px-4 py-3 text-right font-medium">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-800 bg-zinc-900/50">
              {bots.map((bot) => (
                <tr key={bot.id} className="hover:bg-zinc-900">
                  <td className="px-4 py-3 font-mono text-zinc-100">
                    {bot.name}
                  </td>
                  <td className="px-4 py-3 text-zinc-300">
                    {bot.display_name || '-'}
                  </td>
                  <td className="px-4 py-3 font-mono text-xs text-zinc-400">
                    {bot.api_url}
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge isActive={bot.is_active} />
                  </td>
                  <td className="px-4 py-3 text-zinc-400">
                    {dayjs.unix(bot.created_at).format('MMM D, YYYY')}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-2">
                      <button
                        type="button"
                        onClick={() =>
                          toggleMutation.mutate({
                            bot,
                            is_active: !bot.is_active,
                          })
                        }
                        disabled={
                          toggleMutation.isPending &&
                          toggleMutation.variables?.bot.id === bot.id
                        }
                        title={bot.is_active ? 'Deactivate' : 'Activate'}
                        aria-label={`Toggle ${bot.name}`}
                        className="rounded-md border border-zinc-700 p-1.5 text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-200 disabled:opacity-50"
                      >
                        <Power className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => testMutation.mutate(bot.id)}
                        disabled={
                          testMutation.isPending &&
                          testMutation.variables === bot.id
                        }
                        title="Test connectivity"
                        aria-label={`Test ${bot.name}`}
                        className="rounded-md border border-zinc-700 p-1.5 text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-emerald-300 disabled:opacity-50"
                      >
                        {testMutation.isPending &&
                        testMutation.variables === bot.id ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Zap className="h-4 w-4" />
                        )}
                      </button>
                      <button
                        type="button"
                        onClick={() => setEditTarget(bot)}
                        title="Edit"
                        aria-label={`Edit ${bot.name}`}
                        className="rounded-md border border-zinc-700 p-1.5 text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-200"
                      >
                        <Pencil className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => setDeleteTarget(bot)}
                        title="Delete"
                        aria-label={`Delete ${bot.name}`}
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

      {createOpen && (
        <BotFormModal
          mode="create"
          isPending={createMutation.isPending}
          submitError={createMutation.error}
          onClose={() => setCreateOpen(false)}
          onSubmit={(payload) => createMutation.mutate(payload)}
        />
      )}

      {editTarget && (
        <BotFormModal
          mode="edit"
          bot={editTarget}
          isPending={updateMutation.isPending}
          submitError={updateMutation.error}
          onClose={() => setEditTarget(null)}
          onSubmit={(payload) => {
            const body: Parameters<typeof updateBot>[1] = {
              display_name: payload.display_name,
              api_url: payload.api_url,
              system_prompt: payload.system_prompt,
              model: payload.model,
            }
            if (payload.api_key !== '') body.api_key = payload.api_key
            updateMutation.mutate({ id: editTarget.id, body })
          }}
        />
      )}

      {deleteTarget && (
        <ConfirmDeleteDialog
          name={deleteTarget.name}
          isPending={deleteMutation.isPending}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => deleteMutation.mutate(deleteTarget.id)}
        />
      )}
    </div>
  )
}

// --- Create / Edit modal --------------------------------------------------

interface FormPayload {
  name: string
  display_name: string
  api_url: string
  api_key: string
  system_prompt: string
  model: string
}

interface BotFormModalProps {
  mode: 'create' | 'edit'
  bot?: Bot
  isPending: boolean
  submitError: unknown
  onClose: () => void
  onSubmit: (payload: FormPayload) => void
}

const FIELD_CLASS =
  'w-full rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 transition-all focus:border-indigo-500/50 focus:outline-none focus:ring-2 focus:ring-indigo-500/50'

function BotFormModal({
  mode,
  bot,
  isPending,
  submitError,
  onClose,
  onSubmit,
}: BotFormModalProps) {
  const [name, setName] = useState(bot?.name ?? '')
  const [displayName, setDisplayName] = useState(bot?.display_name ?? '')
  const [apiUrl, setApiUrl] = useState(bot?.api_url ?? '')
  const [apiKey, setApiKey] = useState('')
  const [systemPrompt, setSystemPrompt] = useState(bot?.system_prompt ?? '')
  const [model, setModel] = useState(bot?.model ?? 'hermes')

  const trimmedName = name.trim()
  const trimmedUrl = apiUrl.trim()
  const canSubmit =
    trimmedName.length >= 1 && trimmedUrl.length >= 1 && !isPending

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!canSubmit) return
    const payload: FormPayload = {
      name: trimmedName,
      display_name: displayName.trim(),
      api_url: trimmedUrl,
      api_key: apiKey,
      system_prompt: systemPrompt,
      model: model.trim() || 'hermes',
    }
    onSubmit(payload)
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-2xl border border-zinc-800 bg-zinc-950 shadow-2xl shadow-black/50">
        <div className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
          <h2 className="text-lg font-semibold text-zinc-100">
            {mode === 'create' ? 'New Bot' : `Edit ${bot?.name}`}
          </h2>
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
              htmlFor="bot-name"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              Name
            </label>
            <input
              id="bot-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              disabled={mode === 'edit'}
              placeholder="hermes"
              className={FIELD_CLASS}
            />
            {mode === 'edit' && (
              <p className="mt-1 text-xs text-zinc-500">
                Name cannot be changed after creation.
              </p>
            )}
          </div>

          <div>
            <label
              htmlFor="bot-display-name"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              Display Name
            </label>
            <input
              id="bot-display-name"
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Hermes Assistant"
              className={FIELD_CLASS}
            />
          </div>

          <div>
            <label
              htmlFor="bot-api-url"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              API URL
            </label>
            <input
              id="bot-api-url"
              type="url"
              value={apiUrl}
              onChange={(e) => setApiUrl(e.target.value)}
              required
              placeholder="https://hermes.example.com"
              className={FIELD_CLASS}
            />
          </div>

          <div>
            <label
              htmlFor="bot-api-key"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              API Key
            </label>
            <input
              id="bot-api-key"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={
                mode === 'edit' ? '\u2022\u2022\u2022\u2022\u2022' : 'sk-...'
              }
              autoComplete="new-password"
              className={FIELD_CLASS}
            />
            {mode === 'edit' && (
              <p className="mt-1 text-xs text-zinc-500">
                Leave blank to keep the existing key.
              </p>
            )}
          </div>

          <div>
            <label
              htmlFor="bot-model"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              Model
            </label>
            <input
              id="bot-model"
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="hermes"
              className={FIELD_CLASS}
            />
          </div>

          <div>
            <label
              htmlFor="bot-system-prompt"
              className="mb-1.5 block text-sm font-medium text-zinc-300"
            >
              System Prompt
            </label>
            <textarea
              id="bot-system-prompt"
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              rows={4}
              placeholder="You are a helpful assistant..."
              className={`${FIELD_CLASS} resize-y`}
            />
          </div>

          {submitError instanceof Error && (
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
              id="bot-submit"
              type="submit"
              disabled={!canSubmit}
              className="flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isPending ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {mode === 'create' ? 'Creating...' : 'Saving...'}
                </>
              ) : mode === 'create' ? (
                'Create'
              ) : (
                'Save'
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
  name: string
  isPending: boolean
  onCancel: () => void
  onConfirm: () => void
}

function ConfirmDeleteDialog({
  name,
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
          <h2 className="text-lg font-semibold text-zinc-100">Delete Bot</h2>
        </div>
        <div className="px-6 py-4">
          <p className="text-sm text-zinc-300">
            Are you sure you want to delete bot{' '}
            <span className="font-mono text-white">{name}</span>? This action
            cannot be undone.
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
