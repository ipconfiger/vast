import { create } from 'zustand'
import type { ReactNode } from 'react'

export type ToastType = 'success' | 'error' | 'info' | 'warning'

export interface Toast {
  id: string
  type: ToastType
  message: string
  action?: ReactNode
  duration?: number
}

interface ToastState {
  toasts: Toast[]
  addToast: (toast: Omit<Toast, 'id'>) => void
  removeToast: (id: string) => void
}

let toastId = 0

export const useToastStore = create<ToastState>()((set) => ({
  toasts: [],
  addToast: (toast) => {
    const id = `toast-${++toastId}`
    set((state) => ({
      toasts: [...state.toasts, { ...toast, id }],
    }))
    const duration = toast.duration ?? 5000
    if (duration > 0) {
      setTimeout(() => {
        set((state) => ({
          toasts: state.toasts.filter((t) => t.id !== id),
        }))
      }, duration)
    }
  },
  removeToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),
}))

// Convenience helpers
export const toast = {
  success: (message: string) =>
    useToastStore.getState().addToast({ type: 'success', message }),
  error: (message: string) =>
    useToastStore.getState().addToast({ type: 'error', message, duration: 8000 }),
  info: (message: string) =>
    useToastStore.getState().addToast({ type: 'info', message }),
  warning: (message: string) =>
    useToastStore.getState().addToast({ type: 'warning', message, duration: 6000 }),
}
