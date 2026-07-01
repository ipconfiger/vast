import { create } from 'zustand'
import type { Channel } from '../types'

interface ChannelState {
  channels: Channel[]
  currentChannelId: string | null
  setChannels: (channels: Channel[]) => void
  setCurrentChannel: (channelId: string | null) => void
  addChannel: (channel: Channel) => void
  updateChannel: (id: string, updates: Partial<Channel>) => void
  removeChannel: (id: string) => void
}

export const useChannelStore = create<ChannelState>()((set) => ({
  channels: [],
  currentChannelId: null,
  setChannels: (channels) => set({ channels }),
  setCurrentChannel: (channelId) => set({ currentChannelId: channelId }),
  addChannel: (channel) =>
    set((state) => ({ channels: [...state.channels, channel] })),
  updateChannel: (id, updates) =>
    set((state) => ({
      channels: state.channels.map((c) => (c.id === id ? { ...c, ...updates } : c)),
    })),
  removeChannel: (id) =>
    set((state) => ({
      channels: state.channels.filter((c) => c.id !== id),
    })),
}))
