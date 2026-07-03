import { useQuery, useMutation } from '@tanstack/react-query'
import { apiClient } from './client'
import type { Message, Train } from '../types'

/**
 * Fetch a train record (title + replies) by id.
 *
 * `GET /api/trains/:train_id` → `Train`
 */
export function useTrain(trainId: string) {
  return useQuery({
    queryKey: ['train', trainId],
    queryFn: () => apiClient<Train>(`/trains/${trainId}`),
  })
}

interface JoinTrainResult {
  message: Message
  train: Train
}

/**
 * Append a reply to a train.
 *
 * `POST /api/trains/:train_id/join` with `{ content }`
 * → `{ message, train }`
 *
 * The component does not need to invalidate the train query itself —
 * the backend broadcasts a `train_updated` WS event which
 * `useCursorSync` listens to and invalidates `['train', trainId]`.
 */
export function useJoinTrain() {
  return useMutation({
    mutationFn: ({ trainId, content }: { trainId: string; content: string }) =>
      apiClient<JoinTrainResult>(`/trains/${trainId}/join`, {
        method: 'POST',
        body: JSON.stringify({ content }),
      }),
  })
}
