import { create } from 'zustand'
import type { Reaction } from '../types'

interface ReactionState {
  reactionsByMessage: Map<string, Reaction[]>
  setReactions: (messageId: string, reactions: Reaction[]) => void
  addReaction: (messageId: string, reaction: Reaction) => void
  removeReaction: (messageId: string, reactionId: string) => void
}

export const useReactionStore = create<ReactionState>()((set) => ({
  reactionsByMessage: new Map(),

  setReactions: (messageId, reactions) =>
    set((state) => {
      const next = new Map(state.reactionsByMessage)
      next.set(messageId, reactions)
      return { reactionsByMessage: next }
    }),

  addReaction: (messageId, reaction) =>
    set((state) => {
      const next = new Map(state.reactionsByMessage)
      const existing = next.get(messageId) ?? []
      if (existing.some((r) => r.id === reaction.id)) {
        return state
      }
      next.set(messageId, [...existing, reaction])
      return { reactionsByMessage: next }
    }),

  removeReaction: (messageId, reactionId) =>
    set((state) => {
      const next = new Map(state.reactionsByMessage)
      const existing = next.get(messageId)
      if (!existing) return state
      next.set(
        messageId,
        existing.filter((r) => r.id !== reactionId),
      )
      return { reactionsByMessage: next }
    }),
}))
