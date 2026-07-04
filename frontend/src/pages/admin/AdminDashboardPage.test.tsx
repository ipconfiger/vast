import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement } from 'react'
import AdminDashboardPage from './AdminDashboardPage'
import type { DashboardStats } from '../../api/admin'

// --- Mocks -----------------------------------------------------------------

const getDashboardMock = vi.fn()
vi.mock('../../api/admin', () => ({
  getDashboard: (...args: unknown[]) => getDashboardMock(...args),
}))

// --- Fixtures --------------------------------------------------------------

const TEST_STATS: DashboardStats = {
  total_users: 1234,
  active_sessions_24h: 56,
  total_channels: 78,
  total_messages: 9012,
  total_invite_codes: 5,
  active_invite_codes: 3,
}

// --- Helpers ---------------------------------------------------------------

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      {createElement(AdminDashboardPage)}
    </QueryClientProvider>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminDashboardPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders loading skeletons while fetching', () => {
    // Never-resolving promise keeps the query in loading state.
    getDashboardMock.mockReturnValue(new Promise(() => {}))

    const { container } = renderPage()

    // 6 skeleton cards with pulse animation
    expect(container.querySelectorAll('.animate-pulse')).toHaveLength(6)
  })

  it('renders all 6 stat cards with correct formatted values', async () => {
    getDashboardMock.mockResolvedValue(TEST_STATS)

    const { findByText, container } = renderPage()

    // Numbers are formatted with toLocaleString (commas)
    expect(await findByText('1,234')).toBeInTheDocument()
    expect(container.textContent).toContain('56')
    expect(container.textContent).toContain('78')
    expect(container.textContent).toContain('9,012')
    expect(container.textContent).toContain('5')
    expect(container.textContent).toContain('3')

    // Labels are present
    expect(container.textContent).toContain('Total Users')
    expect(container.textContent).toContain('Active Sessions (24h)')
    expect(container.textContent).toContain('Total Channels')
    expect(container.textContent).toContain('Total Messages')
    expect(container.textContent).toContain('Total Invite Codes')
    expect(container.textContent).toContain('Active Invite Codes')
  })

  it('shows error message and retry button when fetch fails', async () => {
    getDashboardMock.mockRejectedValue(new Error('Network error'))

    const { findByText, getByText } = renderPage()

    expect(await findByText('Network error')).toBeInTheDocument()
    expect(getByText('Retry')).toBeInTheDocument()
  })

  it('calls getDashboard exactly once on mount', async () => {
    getDashboardMock.mockResolvedValue(TEST_STATS)

    renderPage()

    await waitFor(() => {
      expect(getDashboardMock).toHaveBeenCalledTimes(1)
    })
  })
})
