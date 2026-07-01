import { create } from 'zustand'

interface PresenceState {
  onlineUsers: Set<string>
  typingUsers: Map<string, Set<string>>
  setOnline: (userIds: string[]) => void
  addOnline: (userId: string) => void
  removeOnline: (userId: string) => void
  setTyping: (channelId: string, userIds: string[]) => void
  addTyping: (channelId: string, userId: string) => void
  removeTyping: (channelId: string, userId: string) => void
}

export const usePresenceStore = create<PresenceState>()((set) => ({
  onlineUsers: new Set(),
  typingUsers: new Map(),

  setOnline: (userIds) => set({ onlineUsers: new Set(userIds) }),

  addOnline: (userId) =>
    set((state) => {
      const next = new Set(state.onlineUsers)
      next.add(userId)
      return { onlineUsers: next }
    }),

  removeOnline: (userId) =>
    set((state) => {
      const next = new Set(state.onlineUsers)
      next.delete(userId)
      return { onlineUsers: next }
    }),

  setTyping: (channelId, userIds) =>
    set((state) => {
      const next = new Map(state.typingUsers)
      next.set(channelId, new Set(userIds))
      return { typingUsers: next }
    }),

  addTyping: (channelId, userId) =>
    set((state) => {
      const next = new Map(state.typingUsers)
      const existing = next.get(channelId) ?? new Set()
      existing.add(userId)
      next.set(channelId, new Set(existing))
      return { typingUsers: next }
    }),

  removeTyping: (channelId, userId) =>
    set((state) => {
      const next = new Map(state.typingUsers)
      const existing = next.get(channelId)
      if (!existing) return state
      existing.delete(userId)
      next.set(channelId, new Set(existing))
      return { typingUsers: next }
    }),
}))
