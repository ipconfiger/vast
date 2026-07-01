import { createPortal } from 'react-dom'
import { useToastStore } from '../stores/toastStore'
import { CheckCircle, XCircle, Info, AlertTriangle, X } from 'lucide-react'

const iconMap = {
  success: CheckCircle,
  error: XCircle,
  info: Info,
  warning: AlertTriangle,
} as const

const colorMap = {
  success: {
    bg: 'border-emerald-500/30 bg-emerald-950/80',
    icon: 'text-emerald-400',
    text: 'text-emerald-100',
    bar: 'bg-emerald-500',
  },
  error: {
    bg: 'border-red-500/30 bg-red-950/80',
    icon: 'text-red-400',
    text: 'text-red-100',
    bar: 'bg-red-500',
  },
  info: {
    bg: 'border-blue-500/30 bg-blue-950/80',
    icon: 'text-blue-400',
    text: 'text-blue-100',
    bar: 'bg-blue-500',
  },
  warning: {
    bg: 'border-amber-500/30 bg-amber-950/80',
    icon: 'text-amber-400',
    text: 'text-amber-100',
    bar: 'bg-amber-500',
  },
}

export function ToastContainer() {
  const toasts = useToastStore((s) => s.toasts)
  const removeToast = useToastStore((s) => s.removeToast)

  if (toasts.length === 0) return null

  return createPortal(
    <div
      aria-live="polite"
      className="pointer-events-none fixed top-4 right-4 z-50 flex flex-col gap-2"
    >
      {toasts.map((toast) => {
        const Icon = iconMap[toast.type]
        const colors = colorMap[toast.type]

        return (
          <div
            key={toast.id}
            className={`pointer-events-auto flex w-80 items-start gap-3 rounded-lg border p-3 shadow-lg backdrop-blur-sm animate-in ${colors.bg}`}
            role="alert"
          >
            <Icon className={`h-5 w-5 flex-shrink-0 ${colors.icon}`} />
            <p className={`flex-1 text-sm ${colors.text}`}>{toast.message}</p>
            {toast.action && <div className="flex-shrink-0">{toast.action}</div>}
            <button
              onClick={() => removeToast(toast.id)}
              className="flex-shrink-0 rounded p-0.5 text-zinc-500 transition-colors hover:text-zinc-300"
              aria-label="Dismiss"
            >
              <X className="h-4 w-4" />
            </button>
            {/* Progress bar */}
            <div className="absolute bottom-0 left-0 h-0.5 w-full overflow-hidden rounded-b-lg">
              <div
                className={`h-full ${colors.bar} animate-toast-progress rounded-full`}
              />
            </div>
          </div>
        )
      })}
    </div>,
    document.body,
  )
}
