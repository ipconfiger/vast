import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, cleanup, fireEvent } from '@testing-library/react'
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
  useAuthStore: vi.fn((selector: (s: { user: { username: string; id: string } | null }) => unknown) =>
    selector({ user: { username: 'me', id: 'u-1' } }),
  ),
}))



// Mutable test state for ChannelSidebar archived-section tests.
let mockChannelStoreChannels: Channel[] = []
const mockDownloadArchive = vi.fn()
vi.mock('../stores/channelStore', () => ({
  useChannelStore: vi.fn((selector: (s: { channels: Channel[]; setChannels: () => void; setCurrentChannel: () => void }) => unknown) =>
    selector({
      channels: mockChannelStoreChannels,
      setChannels: vi.fn(),
      setCurrentChannel: vi.fn(),
    }),
  ),
}))

vi.mock('../stores/presenceStore', () => ({
  usePresenceStore: vi.fn((selector: (s: { onlineUsers: Set<string> }) => unknown) =>
    selector({ onlineUsers: new Set() }),
  ),
}))

vi.mock('../stores/userStore', () => ({
  useUserStore: vi.fn((selector: (s: { getName: () => string }) => unknown) =>
    selector({ getName: vi.fn() }),
  ),
}))

vi.mock('../api/channels', () => ({
  useChannels: () => ({ data: [], isLoading: false }),
  useCreateChannel: () => ({ mutate: vi.fn(), isPending: false }),
  downloadChannelArchive: (...args: unknown[]) => mockDownloadArchive(...args),
}))

vi.mock('../api/dm', () => ({
  useDms: () => ({ data: [] }),
  useCloseDm: () => ({ mutate: vi.fn() }),
}))

vi.mock('react-router', () => ({
  useNavigate: () => vi.fn(),
  useParams: () => ({ channelId: 'ch-1' }),
}))

vi.mock('../hooks/useAuthImage', () => ({
  useAuthImage: () => null,
}))

vi.mock('./Skeletons', () => ({
  ChannelListSkeleton: () => null,
}))

vi.mock('./EmptyState', () => ({
  NoChannelsEmpty: () => null,
}))

vi.mock('./CreateChannelDialog', () => ({
  CreateChannelDialog: () => null,
}))

vi.mock('./DiscoverChannelsModal', () => ({
  DiscoverChannelsModal: () => null,
}))

import { ChannelItem, DmItem, ChannelSidebar } from './ChannelSidebar'

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

describe('ChannelSidebar archived section', () => {
  beforeEach(() => {
    mockChannelStoreChannels = []
    mockDownloadArchive.mockReset()
    mockDownloadArchive.mockResolvedValue(undefined)
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('renders archived channels in separate section with badge', () => {
    mockChannelStoreChannels = [
      makeChannel({ id: 'ch-1', name: 'general', is_archived: false }),
      makeChannel({ id: 'ch-2', name: 'old-project', is_archived: true }),
      makeChannel({ id: 'ch-3', name: 'archived-chat', is_archived: true }),
    ]
    const { getByText, getAllByText, queryAllByText } = render(createElement(ChannelSidebar))
    expect(getAllByText('Archived').length).toBeGreaterThanOrEqual(1)
    expect(getByText('old-project')).toBeTruthy()
    expect(getByText('archived-chat')).toBeTruthy()
    // Each archived channel has an "Archived" badge
    expect(queryAllByText('Archived').length).toBeGreaterThanOrEqual(2)
  })

  it('does not render archived section when no archived channels', () => {
    mockChannelStoreChannels = [
      makeChannel({ id: 'ch-1', name: 'general', is_archived: false }),
    ]
    const { queryByText } = render(createElement(ChannelSidebar))
    expect(queryByText('Archived')).toBeNull()
  })

  it('displays Archive icon next to each archived channel', () => {
    mockChannelStoreChannels = [
      makeChannel({ id: 'ch-1', name: 'general', is_archived: false }),
      makeChannel({ id: 'ch-2', name: 'old-project', is_archived: true }),
    ]
    const { container } = render(createElement(ChannelSidebar))
    // Archive icon is rendered via lucide-react Archive component
    const archiveSection = container.querySelector('.border-t')
    expect(archiveSection?.querySelector('svg')).toBeTruthy()
  })

  it('clicking archived channel calls downloadChannelArchive', () => {
    mockChannelStoreChannels = [
      makeChannel({ id: 'ch-1', name: 'general', is_archived: false }),
      makeChannel({ id: 'ch-archived', name: 'old-project', is_archived: true }),
    ]
    const { getByText } = render(createElement(ChannelSidebar))
    fireEvent.click(getByText('old-project'))
    expect(mockDownloadArchive).toHaveBeenCalledWith('ch-archived', 'old-project')
  })
})
