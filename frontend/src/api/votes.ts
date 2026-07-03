import { useQuery, useMutation } from '@tanstack/react-query'
import { apiClient } from './client'
import type { Message, Vote } from '../types'

/**
 * Fetch a vote record (title + options + counts + myVote) by id.
 *
 * `GET /api/votes/:vote_id` → `Vote`
 */
export function useVote(voteId: string) {
  return useQuery({
    queryKey: ['vote', voteId],
    queryFn: () => apiClient<Vote>(`/votes/${voteId}`),
  })
}

interface CastVoteResult {
  message: Message
  vote: Vote
}

/**
 * Cast a vote for `optionId`.
 *
 * `POST /api/votes/:vote_id/vote` with `{ optionId }`
 * → `{ message, vote }`
 *
 * The component does not need to invalidate the vote query itself —
 * the backend broadcasts a `vote_updated` WS event which
 * `useCursorSync` listens to and invalidates `['vote', voteId]`.
 */
export function useCastVote() {
  return useMutation({
    mutationFn: ({ voteId, optionId }: { voteId: string; optionId: string }) =>
      apiClient<CastVoteResult>(`/votes/${voteId}/vote`, {
        method: 'POST',
        body: JSON.stringify({ optionId }),
      }),
  })
}
