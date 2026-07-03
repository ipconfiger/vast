import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement, type ReactNode } from 'react'
import { useTrain, useJoinTrain } from './trains'
import { useAuthStore } from '../stores/authStore'
import type { Train } from '../types'

// Mock apiClient so the hooks are the only thing under test.
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

const sampleTrain: Train = {
  id: 'train-1',
  channel_id: 'ch-1',
  creator_id: 'user-1',
  title: '午餐接龙',
  replies: [],
  created_at: 1700000000,
}

describe('useTrain', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setToken('test-access-token')
  })

  afterEach(() => {
    setToken(null)
  })

  it('fetches the train via GET /trains/:id through apiClient', async () => {
    apiClientMock.mockResolvedValueOnce(sampleTrain)

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useTrain('train-1'), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(fetchSpy).not.toHaveBeenCalled()
    expect(apiClientMock).toHaveBeenCalledTimes(1)
    const [endpoint, options] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/trains/train-1')
    // GET by default — no method/body required
    expect((options as RequestInit | undefined)?.method).toBeUndefined()
    expect(result.current.data).toEqual(sampleTrain)
  })

  it('uses the correct query key per trainId', async () => {
    apiClientMock.mockResolvedValue(sampleTrain)

    const { wrapper, queryClient } = wrapWithQueryClient()
    const { result } = renderHook(() => useTrain('train-99'), { wrapper })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(queryClient.getQueryData(['train', 'train-99'])).toEqual(sampleTrain)
  })
})

describe('useJoinTrain', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setToken('test-access-token')
  })

  afterEach(() => {
    setToken(null)
  })

  it('POSTs { content } to /trains/:id/join through apiClient', async () => {
    const joinedTrain: Train = {
      ...sampleTrain,
      replies: [
        {
          user_id: 'user-2',
          username: 'alice',
          display_name: null,
          content: '+1',
          created_at: 1700000010,
        },
      ],
    }
    apiClientMock.mockResolvedValueOnce({
      message: { id: 'msg-1', payload: { _train: true } },
      train: joinedTrain,
    })

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useJoinTrain(), { wrapper })

    const res = await result.current.mutateAsync({ trainId: 'train-1', content: '+1' })

    expect(fetchSpy).not.toHaveBeenCalled()
    expect(apiClientMock).toHaveBeenCalledTimes(1)
    const [endpoint, options] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/trains/train-1/join')
    expect((options as RequestInit).method).toBe('POST')
    expect(JSON.parse((options as RequestInit).body as string)).toEqual({ content: '+1' })
    expect(res.train.replies).toHaveLength(1)
    expect(res.train.replies[0].content).toBe('+1')
  })

  it('does NOT hardcode /api/ — uses apiClient API_BASE', async () => {
    apiClientMock.mockResolvedValueOnce({ message: {}, train: sampleTrain })

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useJoinTrain(), { wrapper })

    await result.current.mutateAsync({ trainId: 't-2', content: '我要辣翅' })

    const [endpoint] = apiClientMock.mock.calls[0]
    expect(endpoint).toBe('/trains/t-2/join')
    expect(endpoint).not.toContain('/api/trains')
  })

  it('rethrows ApiClientError on conflict (duplicate join)', async () => {
    const err = new (class TestErr extends Error {
      code = 'CONFLICT'
      status = 409
      name = 'ApiClientError'
    })('You have already joined this train')
    apiClientMock.mockRejectedValueOnce(err)

    const { wrapper } = wrapWithQueryClient()
    const { result } = renderHook(() => useJoinTrain(), { wrapper })

    await waitFor(() => expect(result.current.mutate).toBeDefined())
    await expect(
      result.current.mutateAsync({ trainId: 'train-1', content: '+1' }),
    ).rejects.toThrow('You have already joined this train')
  })
})
