import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { createElement } from 'react'
import type { Train } from '../types'
import { useAuthStore } from '../stores/authStore'

// Mock the train hooks so the test asserts on component logic only.
// API mechanics are covered by trains.test.ts.
const useTrainMock = vi.fn()
const useJoinTrainMock = vi.fn()
vi.mock('../api/trains', () => ({
  useTrain: (...args: unknown[]) => useTrainMock(...args),
  useJoinTrain: () => useJoinTrainMock(),
}))

// Stub the modal so its internals don't interfere with TrainMessage tests.
vi.mock('./TrainRepliesModal', () => ({
  TrainRepliesModal: () => createElement('div', { 'data-testid': 'train-replies-modal' }),
}))

import { TrainMessage } from './TrainMessage'

function setUser(userId: string) {
  useAuthStore.setState({
    token: 'test-access-token',
    isAuthenticated: true,
    user: {
      id: userId,
      username: 'me',
      display_name: 'Me',
      avatar_url: '',
      created_at: '',
    },
  })
}

function makeTrain(overrides: Partial<Train> = {}): Train {
  return {
    id: 'train-1',
    channel_id: 'ch-1',
    creator_id: 'creator-1',
    title: '午餐接龙',
    replies: [],
    created_at: 1700000000,
    ...overrides,
  }
}

function makeMutation() {
  const handlers = {
    mutateAsync: vi.fn().mockResolvedValue({ message: {}, train: {} }),
  }
  return {
    mutateAsync: handlers.mutateAsync,
    mutate: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  }
}

function renderCard(props: { trainId?: string; title?: string; channelId?: string }) {
  return render(
    createElement(TrainMessage, {
      trainId: props.trainId ?? 'train-1',
      title: props.title ?? '午餐接龙',
      channelId: props.channelId ?? 'ch-1',
    }),
  )
}

describe('TrainMessage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setUser('me-1')
    useJoinTrainMock.mockReturnValue(makeMutation())
  })

  afterEach(() => {
    useAuthStore.setState({ user: null, token: null, isAuthenticated: false })
  })

  it('renders the train title', () => {
    useTrainMock.mockReturnValue({ data: makeTrain({ title: '中午去吃肯德基' }), isLoading: false, isError: false })
    const { getByText } = renderCard({ title: '中午去吃肯德基' })
    expect(getByText('中午去吃肯德基')).toBeTruthy()
  })

  it('shows only the last 3 replies when more than 3 exist', () => {
    const replies = [
      { user_id: 'u1', username: 'alice', content: '我要辣翅', created_at: 1 },
      { user_id: 'u2', username: 'bob', content: '+1', created_at: 2 },
      { user_id: 'u3', username: 'carol', content: '我要汉堡', created_at: 3 },
      { user_id: 'u4', username: 'dave', content: '可乐', created_at: 4 },
      { user_id: 'u5', username: 'eve', content: '+1', created_at: 5 },
    ]
    useTrainMock.mockReturnValue({ data: makeTrain({ replies }), isLoading: false, isError: false })
    const { getByText, queryByText } = renderCard({})

    // Last 3 (carol, dave, eve) visible
    expect(getByText('carol')).toBeTruthy()
    expect(getByText('dave')).toBeTruthy()
    expect(getByText('eve')).toBeTruthy()
    // Older ones not inline
    expect(queryByText('alice')).toBeNull()
    expect(queryByText('bob')).toBeNull()
  })

  it('renders "查看全部 (N)" when replies.length > 3', () => {
    useTrainMock.mockReturnValue({
      data: makeTrain({
        replies: [
          { user_id: 'u1', username: 'a', content: 'x', created_at: 1 },
          { user_id: 'u2', username: 'b', content: 'x', created_at: 2 },
          { user_id: 'u3', username: 'c', content: 'x', created_at: 3 },
          { user_id: 'u4', username: 'd', content: 'x', created_at: 4 },
        ],
      }),
      isLoading: false,
      isError: false,
    })
    const { getByText } = renderCard({})
    expect(getByText(/查看全部.*4/)).toBeTruthy()
  })

  it('does NOT render "查看全部" when replies.length <= 3', () => {
    useTrainMock.mockReturnValue({
      data: makeTrain({
        replies: [
          { user_id: 'u1', username: 'a', content: 'x', created_at: 1 },
          { user_id: 'u2', username: 'b', content: 'x', created_at: 2 },
        ],
      }),
      isLoading: false,
      isError: false,
    })
    const { queryByText } = renderCard({})
    expect(queryByText(/查看全部/)).toBeNull()
  })

  it('shows enabled "+ 加入接龙" when the current user has NOT joined', () => {
    useTrainMock.mockReturnValue({
      data: makeTrain({
        replies: [{ user_id: 'someone-else', username: 'bob', content: '+1', created_at: 1 }],
      }),
      isLoading: false,
      isError: false,
    })
    const { getByText } = renderCard({})
    const btn = getByText('+ 加入接龙')
    expect((btn as HTMLButtonElement).disabled).toBe(false)
  })

  it('shows disabled "✓ 已接龙" when the current user HAS joined', () => {
    useTrainMock.mockReturnValue({
      data: makeTrain({
        replies: [{ user_id: 'me-1', username: 'me', content: '+1', created_at: 1 }],
      }),
      isLoading: false,
      isError: false,
    })
    const { getByText, queryByText } = renderCard({})
    const btn = getByText('✓ 已接龙')
    expect((btn as HTMLButtonElement).disabled).toBe(true)
    expect(queryByText('+ 加入接龙')).toBeNull()
  })

  it('reveals an inline textarea when "+ 加入接龙" is clicked', () => {
    useTrainMock.mockReturnValue({
      data: makeTrain({ replies: [] }),
      isLoading: false,
      isError: false,
    })
    const { getByText, queryByPlaceholderText } = renderCard({})

    expect(queryByPlaceholderText(/输入接龙内容/)).toBeNull()
    fireEvent.click(getByText('+ 加入接龙'))
    expect(queryByPlaceholderText(/输入接龙内容/)).toBeTruthy()
  })

  it('calls joinTrain.mutateAsync({ trainId, content }) on submit', async () => {
    const mutate = makeMutation()
    useJoinTrainMock.mockReturnValue(mutate)
    useTrainMock.mockReturnValue({
      data: makeTrain({ replies: [] }),
      isLoading: false,
      isError: false,
    })
    const { getByText, getByPlaceholderText } = renderCard({})

    fireEvent.click(getByText('+ 加入接龙'))
    const input = getByPlaceholderText(/输入接龙内容/) as HTMLTextAreaElement
    fireEvent.change(input, { target: { value: '我要鸡腿' } })
    fireEvent.click(getByText('提交'))

    await waitFor(() => {
      expect(mutate.mutateAsync).toHaveBeenCalledWith({
        trainId: 'train-1',
        content: '我要鸡腿',
      })
    })
  })

  it('renders a loading state while train data is loading', () => {
    useTrainMock.mockReturnValue({ data: undefined, isLoading: true, isError: false })
    const { container } = renderCard({})
    expect(container.querySelector('.animate-spin')).toBeTruthy()
  })

  it('renders an error message when the train fetch fails', () => {
    useTrainMock.mockReturnValue({ data: undefined, isLoading: false, isError: true })
    const { getByText } = renderCard({})
    expect(getByText(/接龙加载失败/)).toBeTruthy()
  })
})
