import { create } from 'zustand'

interface UnreadState {
  unreadByChannel: Record<string, number>
  increment: (channelId: string) => void
  clear: (channelId: string) => void
  clearAll: () => void
}

export const useUnreadStore = create<UnreadState>((set) => ({
  unreadByChannel: {},
  increment: (channelId) => set((state) => ({
    unreadByChannel: {
      ...state.unreadByChannel,
      [channelId]: (state.unreadByChannel[channelId] ?? 0) + 1,
    },
  })),
  clear: (channelId) => set((state) => {
    if (!(channelId in state.unreadByChannel)) return state
    const next = { ...state.unreadByChannel }
    delete next[channelId]
    return { unreadByChannel: next }
  }),
  clearAll: () => set({ unreadByChannel: {} }),
}))