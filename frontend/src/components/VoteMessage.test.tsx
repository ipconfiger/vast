import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import { createElement } from 'react'
import type { Vote } from '../types'

const useVoteMock = vi.fn()
const useCastVoteMock = vi.fn()
vi.mock('../api/votes', () => ({
  useVote: (...args: unknown[]) => useVoteMock(...args),
  useCastVote: () => useCastVoteMock(),
}))

import { VoteMessage } from './VoteMessage'

function makeVote(overrides: Partial<Vote> = {}): Vote {
  return {
    id: 'vote-1',
    channelId: 'ch-1',
    creatorId: 'creator-1',
    title: '中午吃啥',
    options: [
      { id: 'opt-a', text: '麦当劳', count: 0 },
      { id: 'opt-b', text: '肯德基', count: 0 },
    ],
    myVote: null,
    createdAt: 1700000000,
    ...overrides,
  }
}

function makeCastVote() {
  return {
    mutate: vi.fn(),
    mutateAsync: vi.fn().mockResolvedValue({}),
    isPending: false,
    isError: false,
    error: null,
  }
}

function renderCard(props: { voteId?: string; title?: string; channelId?: string }) {
  return render(
    createElement(VoteMessage, {
      voteId: props.voteId ?? 'vote-1',
      title: props.title ?? '中午吃啥',
      channelId: props.channelId ?? 'ch-1',
    }),
  )
}

describe('VoteMessage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useCastVoteMock.mockReturnValue(makeCastVote())
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders the card with title', () => {
    useVoteMock.mockReturnValue({ data: makeVote(), isLoading: false, isError: false })
    const { getByText } = renderCard({ title: '今晚吃啥' })
    expect(getByText('今晚吃啥')).toBeTruthy()
  })

  it('shows all options with their text', () => {
    useVoteMock.mockReturnValue({ data: makeVote(), isLoading: false, isError: false })
    const { getByText } = renderCard({})
    expect(getByText('麦当劳')).toBeTruthy()
    expect(getByText('肯德基')).toBeTruthy()
  })

  it('shows correct percentages', () => {
    useVoteMock.mockReturnValue({
      data: makeVote({
        options: [
          { id: 'opt-a', text: '麦当劳', count: 2 },
          { id: 'opt-b', text: '肯德基', count: 1 },
        ],
      }),
      isLoading: false,
      isError: false,
    })
    const { getByText } = renderCard({})
    expect(getByText(/2\s*\(67%\)/)).toBeTruthy()
    expect(getByText(/1\s*\(33%\)/)).toBeTruthy()
  })

  it('enables vote button when myVote is null', () => {
    useVoteMock.mockReturnValue({ data: makeVote(), isLoading: false, isError: false })
    const { getAllByText } = renderCard({})
    const voteButtons = getAllByText('投票')
    expect(voteButtons.length).toBe(2)
    expect((voteButtons[0] as HTMLButtonElement).disabled).toBe(false)
    expect((voteButtons[1] as HTMLButtonElement).disabled).toBe(false)
  })

  it('calls mutate with correct optionId on click', () => {
    const castVote = makeCastVote()
    useCastVoteMock.mockReturnValue(castVote)
    useVoteMock.mockReturnValue({ data: makeVote(), isLoading: false, isError: false })
    const { getAllByText } = renderCard({})

    fireEvent.click(getAllByText('投票')[0])
    expect(castVote.mutate).toHaveBeenCalledWith({ voteId: 'vote-1', optionId: 'opt-a' })
  })

  it('shows "✓ 已投" on the voted option when myVote is set', () => {
    useVoteMock.mockReturnValue({
      data: makeVote({
        options: [
          { id: 'opt-a', text: '麦当劳', count: 1 },
          { id: 'opt-b', text: '肯德基', count: 0 },
        ],
        myVote: 'opt-a',
      }),
      isLoading: false,
      isError: false,
    })
    const { getByText } = renderCard({})
    expect(getByText('✓ 已投')).toBeTruthy()
  })

  it('disables vote buttons on other options when already voted', () => {
    useVoteMock.mockReturnValue({
      data: makeVote({
        options: [
          { id: 'opt-a', text: '麦当劳', count: 1 },
          { id: 'opt-b', text: '肯德基', count: 0 },
        ],
        myVote: 'opt-a',
      }),
      isLoading: false,
      isError: false,
    })
    const { getAllByText } = renderCard({})
    const disabledButtons = getAllByText('投票')
    expect(disabledButtons).toHaveLength(1)
    expect((disabledButtons[0] as HTMLButtonElement).disabled).toBe(true)
  })

  it('displays total vote count in footer', () => {
    useVoteMock.mockReturnValue({
      data: makeVote({
        options: [
          { id: 'opt-a', text: '麦当劳', count: 2 },
          { id: 'opt-b', text: '肯德基', count: 1 },
        ],
      }),
      isLoading: false,
      isError: false,
    })
    const { getByText } = renderCard({})
    expect(getByText(/共\s*3\s*人投票/)).toBeTruthy()
  })

  it('shows all bars empty with 0% when no votes', () => {
    useVoteMock.mockReturnValue({ data: makeVote(), isLoading: false, isError: false })
    const { getAllByText, getByText } = renderCard({})
    expect(getAllByText(/0\s*\(0%\)/)).toHaveLength(2)
    expect(getByText(/共\s*0\s*人投票/)).toBeTruthy()
  })

  it('renders loading state with spinner', () => {
    useVoteMock.mockReturnValue({ data: undefined, isLoading: true, isError: false })
    const { container } = renderCard({})
    expect(container.querySelector('.animate-spin')).toBeTruthy()
  })

  it('renders error message when fetch fails', () => {
    useVoteMock.mockReturnValue({ data: undefined, isLoading: false, isError: true })
    const { getByText } = renderCard({})
    expect(getByText(/投票加载失败/)).toBeTruthy()
  })

  it('does not call mutate when already voted', async () => {
    const castVote = makeCastVote()
    useCastVoteMock.mockReturnValue(castVote)
    useVoteMock.mockReturnValue({
      data: makeVote({ myVote: 'opt-a' }),
      isLoading: false,
      isError: false,
    })
    const { queryAllByText } = renderCard({})
    const enabledButtons = queryAllByText('投票').filter(
      (el) => el.tagName === 'BUTTON' && !(el as HTMLButtonElement).disabled,
    )
    expect(enabledButtons).toHaveLength(0)
    expect(castVote.mutate).not.toHaveBeenCalled()
  })

  it('truncates long option text', () => {
    useVoteMock.mockReturnValue({
      data: makeVote({
        options: [{ id: 'opt-a', text: '这是一个非常非常非常长的选项文本内容', count: 0 }],
      }),
      isLoading: false,
      isError: false,
    })
    const { container } = renderCard({})
    const truncated = container.querySelector('.truncate')
    expect(truncated).toBeTruthy()
  })
})
