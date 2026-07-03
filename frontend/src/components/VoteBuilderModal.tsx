import { useState, useEffect } from 'react'
import { X, Plus } from 'lucide-react'

const MIN_OPTIONS = 2
const MAX_OPTIONS = 10

interface VoteBuilderModalProps {
  isOpen: boolean
  onClose: () => void
  onConfirm: (title: string, options: string[]) => void
  initialTitle: string
}

export function VoteBuilderModal({ isOpen, onClose, onConfirm, initialTitle }: VoteBuilderModalProps) {
  const [title, setTitle] = useState(initialTitle)
  const [options, setOptions] = useState<string[]>(['', ''])

  useEffect(() => {
    if (isOpen) {
      setTitle(initialTitle)
      setOptions(['', ''])
    }
  }, [isOpen, initialTitle])

  if (!isOpen) return null

  const filledOptions = options.filter((o) => o.trim().length > 0)
  const canConfirm =
    title.trim().length > 0 &&
    options.length >= MIN_OPTIONS &&
    filledOptions.length === options.length &&
    filledOptions.length >= MIN_OPTIONS

  const handleAddOption = () => {
    if (options.length >= MAX_OPTIONS) return
    setOptions((prev) => [...prev, ''])
  }

  const handleRemoveOption = (index: number) => {
    if (options.length <= MIN_OPTIONS) return
    setOptions((prev) => prev.filter((_, i) => i !== index))
  }

  const handleOptionChange = (index: number, value: string) => {
    setOptions((prev) => prev.map((o, i) => (i === index ? value : o)))
  }

  const handleConfirm = () => {
    if (!canConfirm) return
    onConfirm(title.trim(), options.map((o) => o.trim()).filter((o) => o.length > 0))
    onClose()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        className="relative w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 p-6 shadow-2xl shadow-black/50"
      >
        {/* Header */}
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold text-zinc-100">发起投票</h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-zinc-500 transition-colors hover:text-zinc-300"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Title input */}
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="投票标题"
          autoFocus
          className="mb-4 w-full rounded-md border border-zinc-600 bg-zinc-900 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/50 focus:outline-none"
        />

        {/* Option inputs */}
        <div className="flex flex-col gap-2">
          {options.map((opt, i) => (
            <div key={i} className="flex items-center gap-2">
              <input
                type="text"
                value={opt}
                onChange={(e) => handleOptionChange(i, e.target.value)}
                placeholder={`选项 ${i + 1}`}
                className="flex-1 rounded-md border border-zinc-600 bg-zinc-900 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/50 focus:outline-none"
              />
              {options.length > MIN_OPTIONS && (
                <button
                  type="button"
                  onClick={() => handleRemoveOption(i)}
                  className="flex-shrink-0 rounded-md p-1.5 text-zinc-500 transition-colors hover:bg-zinc-700 hover:text-zinc-300"
                  aria-label={`移除选项 ${i + 1}`}
                >
                  <X className="h-4 w-4" />
                </button>
              )}
            </div>
          ))}
        </div>

        {/* Add option button */}
        <button
          type="button"
          onClick={handleAddOption}
          disabled={options.length >= MAX_OPTIONS}
          className="mt-3 flex items-center gap-1 rounded-md px-2 py-1 text-xs text-indigo-400 transition-colors hover:text-indigo-300 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Plus className="h-3 w-3" />
          添加选项
        </button>

        {/* Actions */}
        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-xs text-zinc-400 transition-colors hover:text-zinc-200"
          >
            取消
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!canConfirm}
            className="rounded-md bg-indigo-600 px-4 py-1.5 text-xs font-medium text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            确认
          </button>
        </div>
      </div>
    </div>
  )
}
