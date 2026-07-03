import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement, type ReactNode } from 'react'
import { useVote, useCastVote } from './votes'
import { useAuthStore } from '../stores/authStore'
import type { Vote } from '../types'

const apiClientMock = vi.fn()
vi.mock('./client', () => ({
  apiClient: (...args: unknown[]) => apiClientMock(...args),
  ApiClientError: class ApiClientError extends Error {
    code: string
    status: number
    constructor(code: string, message: string, status: number) {
      super(message)
      this.code = code
      this.status = status
      this.name = 'ApiClientError'
    }
  },
}))

const fetchSpy = vi.fn()
vi.stubGlobal('fetch', fetchSpy)

function setToken(token: string | null) {
  useAuthStore.setState({ token, isAuthenticated: token !== null })
}

function wrapWithQueryClient() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: queryClient }, children)
  return { wrapper, queryClient }
}

const sampleVote: Vote = {
  id: 'vote-1',
  channelId: 'ch-1',
  creatorId: 'user-1',
  title: '中午吃啥',
  options: [
    { id: 'opt-a', text: '麦当劳', count: 0 },
    { id: 'opt-b', text: '肯德基', count: 0 },
  ],
  myVote: null,
  createdAt: 1700000000,
}

describe('useVote', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setToken('test-access-token')
  })

  afterEach(() => {
    setToken(null)
  })

  it('fetches the vote via GET /votes/:id through apiClient', async () => {
    apiClientMock.mockResolvedValueOnce(sampleVote)

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useVote('vote-1'), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(fetchSpy).not.toHaveBeenCalled()
    expect(apiClientMock).toHaveBeenCalledTimes(1)
    const [endpoint, options] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/votes/vote-1')
    expect((options as RequestInit | undefined)?.method).toBeUndefined()
    expect(result.current.data).toEqual(sampleVote)
  })

  it('uses the correct query key per voteId', async () => {
    apiClientMock.mockResolvedValue(sampleVote)

    const { wrapper, queryClient } = wrapWithQueryClient()
    const { result } = renderHook(() => useVote('vote-99'), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(queryClient.getQueryData(['vote', 'vote-99'])).toEqual(sampleVote)
  })

  it('does NOT hardcode /api/ — uses apiClient API_BASE', async () => {
    apiClientMock.mockResolvedValueOnce(sampleVote)

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useVote('vote-1'), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    const [endpoint] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/votes/vote-1')
    expect(endpoint).not.toContain('/api/votes')
  })
})

describe('useCastVote', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setToken('test-access-token')
  })

  afterEach(() => {
    setToken(null)
  })

  it('POSTs { optionId } to /votes/:id/vote through apiClient', async () => {
    const votedState: Vote = {
      ...sampleVote,
      options: [
        { id: 'opt-a', text: '麦当劳', count: 1 },
        { id: 'opt-b', text: '肯德基', count: 0 },
      ],
      myVote: 'opt-a',
    }
    apiClientMock.mockResolvedValueOnce({
      message: { id: 'msg-1', payload: { _vote: true } },
      vote: votedState,
    })

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useCastVote(), { wrapper })

    const res = await result.current.mutateAsync({ voteId: 'vote-1', optionId: 'opt-a' })

    expect(fetchSpy).not.toHaveBeenCalled()
    expect(apiClientMock).toHaveBeenCalledTimes(1)
    const [endpoint, options] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/votes/vote-1/vote')
    expect((options as RequestInit).method).toBe('POST')
    expect(JSON.parse((options as RequestInit).body as string)).toEqual({ optionId: 'opt-a' })
    expect(res.vote.myVote).toBe('opt-a')
    expect(res.vote.options[0].count).toBe(1)
  })

  it('rethrows ApiClientError on conflict (duplicate vote)', async () => {
    const err = new (class TestErr extends Error {
      code = 'CONFLICT'
      status = 409
      name = 'ApiClientError'
    })('您已投票')
    apiClientMock.mockRejectedValueOnce(err)

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useCastVote(), { wrapper })

    await waitFor(() => expect(result.current.mutate).toBeDefined())
    await expect(
      result.current.mutateAsync({ voteId: 'vote-1', optionId: 'opt-a' }),
    ).rejects.toThrow('您已投票')
  })
})
