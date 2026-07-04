import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement } from 'react'
import AdminInviteCodesPage from './AdminInviteCodesPage'
import type { InviteCode } from '../../api/admin'

// --- Mocks -----------------------------------------------------------------

const listInviteCodesMock = vi.fn()
const createInviteCodeMock = vi.fn()
const updateInviteCodeMock = vi.fn()
const deleteInviteCodeMock = vi.fn()

vi.mock('../../api/admin', () => ({
  listInviteCodes: (arg?: unknown) => listInviteCodesMock(arg),
  createInviteCode: (arg: unknown) => createInviteCodeMock(arg),
  updateInviteCode: (code: unknown, body: unknown) =>
    updateInviteCodeMock(code, body),
  deleteInviteCode: (code: unknown) => deleteInviteCodeMock(code),
  AdminApiClientError: class AdminApiClientError extends Error {
    code: string
    status: number
    constructor(code: string, message: string, status: number) {
      super(message)
      this.code = code
      this.status = status
      this.name = 'AdminApiClientError'
    }
  },
}))

// --- Fixtures --------------------------------------------------------------

function makeCode(over: Partial<InviteCode> = {}): InviteCode {
  return {
    code: 'WELCOME2026',
    created_by_user_id: null,
    max_uses: 100,
    use_count: 42,
    is_active: true,
    created_at: 1730000000,
    ...over,
  }
}

// --- Helpers ---------------------------------------------------------------

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      {createElement(AdminInviteCodesPage)}
    </QueryClientProvider>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminInviteCodesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Default happy-path: a single active invite code.
    listInviteCodesMock.mockResolvedValue([makeCode()])
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders loading spinner while fetching', () => {
    listInviteCodesMock.mockReturnValue(new Promise(() => {}))
    const { container } = renderPage()
    expect(container.querySelector('.animate-spin')).toBeTruthy()
  })

  it('renders table rows when invite codes are loaded', async () => {
    const { findByText } = renderPage()
    expect(await findByText('WELCOME2026')).toBeInTheDocument()
  })

  it('shows empty state when there are no invite codes', async () => {
    listInviteCodesMock.mockResolvedValue([])
    const { findByText } = renderPage()
    expect(await findByText('No invite codes yet')).toBeInTheDocument()
  })

  it('shows error state when list fetch fails', async () => {
    listInviteCodesMock.mockRejectedValue(new Error('Network down'))
    const { findByText } = renderPage()
    expect(await findByText('Network down')).toBeInTheDocument()
  })

  it('renders usage ratio as use_count/max_uses', async () => {
    const { findByText } = renderPage()
    expect(await findByText('42/100')).toBeInTheDocument()
  })

  it('renders Active badge for an active code', async () => {
    const { findByText } = renderPage()
    expect(await findByText('Active')).toBeInTheDocument()
  })

  it('renders Inactive badge for a disabled code', async () => {
    listInviteCodesMock.mockResolvedValue([makeCode({ is_active: false })])
    const { findByText } = renderPage()
    expect(await findByText('Inactive')).toBeInTheDocument()
  })

  it('opens create modal when "New Invite Code" is clicked', async () => {
    const { findByText, container } = renderPage()
    fireEvent.click(await findByText('New Invite Code'))
    expect(container.querySelector('#ic-code')).toBeTruthy()
    expect(container.querySelector('#ic-max-uses')).toBeTruthy()
    expect(container.querySelector('#ic-active')).toBeTruthy()
  })

  it('creates invite code via modal and refreshes the list', async () => {
    const created = makeCode({ code: 'NEWMAGIC', use_count: 0 })
    createInviteCodeMock.mockResolvedValue(created)
    // First call returns the seeded code; second (post-invalidation) returns both.
    let callCount = 0
    listInviteCodesMock.mockImplementation(() => {
      callCount += 1
      return Promise.resolve(callCount === 1 ? [makeCode()] : [makeCode(), created])
    })

    const { findByText, container } = renderPage()
    fireEvent.click(await findByText('New Invite Code'))

    fireEvent.change(container.querySelector('#ic-code') as HTMLInputElement, {
      target: { value: 'NEWMAGIC' },
    })
    fireEvent.change(container.querySelector('#ic-max-uses') as HTMLInputElement, {
      target: { value: '50' },
    })
    fireEvent.click(container.querySelector('#ic-submit') as HTMLButtonElement)

    await waitFor(() => {
      expect(createInviteCodeMock).toHaveBeenCalledWith({
        code: 'NEWMAGIC',
        max_uses: 50,
        is_active: true,
      })
    })
    // List query is invalidated and refetched after a successful create.
    await waitFor(() => {
      expect(listInviteCodesMock).toHaveBeenCalledTimes(2)
    })
  })

  it('shows inline 409 error when creating a duplicate code', async () => {
    const conflictError = Object.assign(new Error('Code already exists'), {
      code: 'CONFLICT',
      status: 409,
    })
    createInviteCodeMock.mockRejectedValue(conflictError)

    const { findByText, container } = renderPage()
    fireEvent.click(await findByText('New Invite Code'))
    fireEvent.change(container.querySelector('#ic-code') as HTMLInputElement, {
      target: { value: 'DUP' },
    })
    fireEvent.click(container.querySelector('#ic-submit') as HTMLButtonElement)

    await waitFor(() => {
      expect(createInviteCodeMock).toHaveBeenCalled()
    })
    // Inline modal error message (not a toast) — modal stays open.
    expect(await findByText(/already exists/i)).toBeInTheDocument()
  })

  it('toggles active state via the toggle action', async () => {
    updateInviteCodeMock.mockResolvedValue(makeCode({ is_active: false }))

    const { findByLabelText } = renderPage()
    fireEvent.click(await findByLabelText('Toggle WELCOME2026'))

    await waitFor(() => {
      expect(updateInviteCodeMock).toHaveBeenCalledWith('WELCOME2026', {
        is_active: false,
      })
    })
  })

  it('resets use count via the reset action', async () => {
    updateInviteCodeMock.mockResolvedValue(makeCode({ use_count: 0 }))

    const { findByLabelText } = renderPage()
    fireEvent.click(await findByLabelText('Reset WELCOME2026'))

    await waitFor(() => {
      expect(updateInviteCodeMock).toHaveBeenCalledWith('WELCOME2026', {
        reset_use_count: true,
      })
    })
  })

  it('opens a confirm dialog when delete is clicked', async () => {
    const { findByLabelText, findByText } = renderPage()
    fireEvent.click(await findByLabelText('Delete WELCOME2026'))
    expect(await findByText(/Are you sure/i)).toBeInTheDocument()
  })

  it('deletes the invite code after confirming', async () => {
    deleteInviteCodeMock.mockResolvedValue(undefined)

    const { findByLabelText, findByText } = renderPage()
    fireEvent.click(await findByLabelText('Delete WELCOME2026'))
    fireEvent.click(await findByText('Delete'))

    await waitFor(() => {
      expect(deleteInviteCodeMock).toHaveBeenCalledWith('WELCOME2026')
    })
  })
})
