import { create } from 'zustand'
import type { Message } from '../types'

interface MessageState {
  messagesByChannel: Map<string, Message[]>
  lastCursorByChannel: Map<string, string | null>
  setMessages: (channelId: string, messages: Message[], cursor?: string | null) => void
  addMessage: (channelId: string, message: Message) => void
  updateMessage: (channelId: string, messageId: string, updates: Partial<Message>) => void
  removeMessage: (channelId: string, messageId: string) => void
  clearChannel: (channelId: string) => void
}

export const useMessageStore = create<MessageState>()((set) => ({
  messagesByChannel: new Map(),
  lastCursorByChannel: new Map(),

  setMessages: (channelId, messages, cursor = null) =>
    set((state) => {
      const next = new Map(state.messagesByChannel)
      next.set(channelId, messages)
      const cursors = new Map(state.lastCursorByChannel)
      cursors.set(channelId, cursor ?? null)
      return { messagesByChannel: next, lastCursorByChannel: cursors }
    }),

  addMessage: (channelId, message) =>
    set((state) => {
      const next = new Map(state.messagesByChannel)
      const existing = next.get(channelId) ?? []
      if (existing.some((m) => m.msg_id === message.msg_id)) {
        return state
      }
      next.set(channelId, [...existing, message])
      return { messagesByChannel: next }
    }),

  updateMessage: (channelId, messageId, updates) =>
    set((state) => {
      const next = new Map(state.messagesByChannel)
      const existing = next.get(channelId)
      if (!existing) return state
      next.set(
        channelId,
        existing.map((m) => (m.id === messageId || m.msg_id === messageId ? { ...m, ...updates } : m)),
      )
      return { messagesByChannel: next }
    }),

  removeMessage: (channelId, messageId) =>
    set((state) => {
      const next = new Map(state.messagesByChannel)
      const existing = next.get(channelId)
      if (!existing) return state
      next.set(
        channelId,
        existing.filter((m) => m.id !== messageId && m.msg_id !== messageId),
      )
      return { messagesByChannel: next }
    }),

  clearChannel: (channelId) =>
    set((state) => {
      const next = new Map(state.messagesByChannel)
      next.delete(channelId)
      const cursors = new Map(state.lastCursorByChannel)
      cursors.delete(channelId)
      return { messagesByChannel: next, lastCursorByChannel: cursors }
    }),
}))

// E2E testability hook — `import.meta.env.DEV` is statically false in
// production builds, so vite's dead-code elimination drops this entirely.
if (import.meta.env.DEV) {
  ;(window as unknown as { __messageStore?: typeof useMessageStore }).__messageStore = useMessageStore
}
