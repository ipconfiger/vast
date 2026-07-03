import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent, waitFor } from '@testing-library/react'
import { createElement } from 'react'
import type { Train } from '../types'

const useTrainMock = vi.fn()
vi.mock('../api/trains', () => ({
  useTrain: (...args: unknown[]) => useTrainMock(...args),
}))

import { TrainRepliesModal } from './TrainRepliesModal'

function makeTrain(replies: Train['replies']): Train {
  return {
    id: 'train-1',
    channel_id: 'ch-1',
    creator_id: 'creator-1',
    title: '午餐接龙',
    replies,
    created_at: 1700000000,
  }
}

describe('TrainRepliesModal', () => {
  beforeEach(() => vi.clearAllMocks())

  afterEach(() => vi.restoreAllMocks())

  it('returns null when isOpen is false', () => {
    useTrainMock.mockReturnValue({ data: makeTrain([]), isLoading: false, isError: false })
    const { container } = render(
      createElement(TrainRepliesModal, { trainId: 'train-1', isOpen: false, onClose: () => {} }),
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders ALL replies when open', () => {
    const replies = [
      { user_id: 'u1', username: 'alice', display_name: null, content: '我要辣翅', created_at: 1 },
      { user_id: 'u2', username: 'bob', display_name: null, content: '+1', created_at: 2 },
      { user_id: 'u3', username: 'carol', display_name: null, content: '我要汉堡', created_at: 3 },
      { user_id: 'u4', username: 'dave', display_name: null, content: '可乐', created_at: 4 },
      { user_id: 'u5', username: 'eve', display_name: null, content: '+1', created_at: 5 },
    ]
    useTrainMock.mockReturnValue({ data: makeTrain(replies), isLoading: false, isError: false })
    const { getByText, container } = render(
      createElement(TrainRepliesModal, { trainId: 'train-1', isOpen: true, onClose: () => {} }),
    )

    // All five usernames + contents rendered (modal shows everything, not just last 3).
    for (const name of ['alice', 'bob', 'carol', 'dave', 'eve']) {
      expect(getByText(name)).toBeTruthy()
    }
    expect(getByText('我要辣翅')).toBeTruthy()
    expect(getByText('可乐')).toBeTruthy()
    // Header count
    expect(getByText(/5\s*人参与/)).toBeTruthy()
    // Sanity: the modal panel is present
    expect(container.querySelector('[role="dialog"]') ?? container.firstChild).toBeTruthy()
  })

  it('calls onClose when the backdrop is clicked', () => {
    useTrainMock.mockReturnValue({ data: makeTrain([]), isLoading: false, isError: false })
    const onClose = vi.fn()
    const { container } = render(
      createElement(TrainRepliesModal, { trainId: 'train-1', isOpen: true, onClose }),
    )

    // The backdrop is the first child (absolute inset-0 bg-black/60).
    const backdrop = container.querySelector('.bg-black\\/60') as HTMLElement
    expect(backdrop).toBeTruthy()
    fireEvent.click(backdrop)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('calls onClose when the X button is clicked', () => {
    useTrainMock.mockReturnValue({ data: makeTrain([]), isLoading: false, isError: false })
    const onClose = vi.fn()
    const { getByLabelText } = render(
      createElement(TrainRepliesModal, { trainId: 'train-1', isOpen: true, onClose }),
    )

    fireEvent.click(getByLabelText('Close'))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('shows a loading spinner while the train is loading', () => {
    useTrainMock.mockReturnValue({ data: undefined, isLoading: true, isError: false })
    const { container } = render(
      createElement(TrainRepliesModal, { trainId: 'train-1', isOpen: true, onClose: () => {} }),
    )
    expect(container.querySelector('.animate-spin')).toBeTruthy()
  })

  it('shows an error message when the train fetch fails', async () => {
    useTrainMock.mockReturnValue({ data: undefined, isLoading: false, isError: true })
    const { getByText } = render(
      createElement(TrainRepliesModal, { trainId: 'train-1', isOpen: true, onClose: () => {} }),
    )
    await waitFor(() => expect(getByText(/接龙加载失败/)).toBeTruthy())
  })
})
