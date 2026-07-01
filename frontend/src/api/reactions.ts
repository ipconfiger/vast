import { apiClient } from './client'
import type { Reaction } from '../types'

interface ToggleReactionResponse {
  added?: Reaction
  removed?: { reaction_id: string }
}

export async function toggleReaction(
  messageId: string,
  emoji: string,
): Promise<ToggleReactionResponse> {
  return apiClient<ToggleReactionResponse>(`/messages/${messageId}/reactions`, {
    method: 'POST',
    body: JSON.stringify({ emoji }),
  })
}

export async function fetchReactions(messageId: string): Promise<Reaction[]> {
  return apiClient<Reaction[]>(`/messages/${messageId}/reactions`)
}
