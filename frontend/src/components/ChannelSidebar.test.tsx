import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, cleanup } from '@testing-library/react'
import { createElement } from 'react'
import type { Channel } from '../types'
import type { DmChannel } from '../api/dm'

// Per-test mutable store state. Tests call `setUnread(...)` to seed counts.
let mockUnreadByChannel: Record<string, number> = {}
vi.mock('../stores/unreadStore', () => ({
  useUnreadStore: vi.fn((selector: (s: { unreadByChannel: Record<string, number> }) => unknown) =>
    selector({ unreadByChannel: mockUnreadByChannel }),
  ),
}))

// DmItem reads current username from authStore; stub it so dmDisplayName is deterministic.
vi.mock('../stores/authStore', () => ({
  useAuthStore: vi.fn((selector: (s: { user: { username: string } | null }) => unknown) =>
    selector({ user: { username: 'me' } }),
  ),
}))

import { ChannelItem, DmItem } from './ChannelSidebar'

function makeChannel(overrides: Partial<Channel> = {}): Channel {
  return {
    id: 'ch-1',
    name: 'general',
    type: 'public',
    created_by: 'u-1',
    created_at: '',
    is_archived: false,
    ...overrides,
  }
}

function makeDm(overrides: Partial<DmChannel> = {}): DmChannel {
  return {
    id: 'dm-1',
    name: 'alice, me',
    description: '',
    owner_id: null,
    is_direct: true,
    is_group_dm: false,
    is_archived: false,
    created_at: 0,
    ...overrides,
  }
}

describe('ChannelItem unread badge', () => {
  beforeEach(() => {
    mockUnreadByChannel = {}
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('renders badge with the unread count when unread > 0', () => {
    mockUnreadByChannel = { 'ch-1': 3 }
    const { getByText } = render(
      createElement(ChannelItem, {
        channel: makeChannel(),
        isActive: false,
        onClick: () => {},
      }),
    )
    expect(getByText('3')).toBeTruthy()
  })

  it('renders no badge when unread = 0', () => {
    mockUnreadByChannel = { 'ch-1': 0 }
    const { container, queryByText } = render(
      createElement(ChannelItem, {
        channel: makeChannel(),
        isActive: false,
        onClick: () => {},
      }),
    )
    expect(queryByText(/^\d+$/)).toBeNull()
    expect(container.querySelector('.bg-red-500')).toBeNull()
  })

  it('caps the displayed count at "99+" when unread > 99', () => {
    mockUnreadByChannel = { 'ch-1': 150 }
    const { getByText, queryByText } = render(
      createElement(ChannelItem, {
        channel: makeChannel(),
        isActive: false,
        onClick: () => {},
      }),
    )
    expect(getByText('99+')).toBeTruthy()
    expect(queryByText('150')).toBeNull()
  })

  it('hides the badge when isActive is true even if unread > 0 (double-guard)', () => {
    mockUnreadByChannel = { 'ch-1': 5 }
    const { container, queryByText } = render(
      createElement(ChannelItem, {
        channel: makeChannel(),
        isActive: true,
        onClick: () => {},
      }),
    )
    expect(queryByText('5')).toBeNull()
    expect(container.querySelector('.bg-red-500')).toBeNull()
  })
})

describe('DmItem unread badge', () => {
  beforeEach(() => {
    mockUnreadByChannel = {}
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('renders badge with the unread count when unread > 0', () => {
    mockUnreadByChannel = { 'dm-1': 7 }
    const { getByText } = render(
      createElement(DmItem, {
        dm: makeDm(),
        onClick: () => {},
      }),
    )
    expect(getByText('7')).toBeTruthy()
  })

  it('renders no badge when unread = 0', () => {
    mockUnreadByChannel = {}
    const { container, queryByText } = render(
      createElement(DmItem, {
        dm: makeDm(),
        onClick: () => {},
      }),
    )
    expect(queryByText(/^\d+$/)).toBeNull()
    expect(container.querySelector('.bg-red-500')).toBeNull()
  })
})
