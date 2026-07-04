import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement } from 'react'
import dayjs from 'dayjs'
import AdminUsersPage from './AdminUsersPage'
import { AdminApiClientError } from '../../api/admin'
import type { AdminUser } from '../../api/admin'

// --- Mocks -----------------------------------------------------------------

const listUsersMock = vi.fn()
const updateUserMock = vi.fn()
const resetUserPasswordMock = vi.fn()
const deleteUserMock = vi.fn()

// Partial mock: preserve AdminApiClientError class identity (needed for the
// page's `instanceof AdminApiClientError` check in onError) while stubbing
// the four endpoint functions the page exercises.
vi.mock('../../api/admin', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/admin')>()
  return {
    ...actual,
    listUsers: (...args: unknown[]) => listUsersMock(...args),
    updateUser: (...args: unknown[]) => updateUserMock(...args),
    resetUserPassword: (...args: unknown[]) => resetUserPasswordMock(...args),
    deleteUser: (...args: unknown[]) => deleteUserMock(...args),
  }
})

const toastSuccessMock = vi.fn()
const toastErrorMock = vi.fn()
vi.mock('../../stores/toastStore', () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
    info: vi.fn(),
    warning: vi.fn(),
  },
}))

// --- Fixtures --------------------------------------------------------------

const TEST_USERS: AdminUser[] = [
  {
    id: 'user-1',
    username: 'alice',
    display_name: 'Alice Smith',
    avatar_url: '',
    created_at: 1_700_000_000,
  },
  {
    id: 'user-2',
    username: 'bob',
    display_name: 'Bob Jones',
    avatar_url: '',
    created_at: 1_700_000_500,
  },
]

// --- Helpers ---------------------------------------------------------------

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      {createElement(AdminUsersPage)}
    </QueryClientProvider>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminUsersPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listUsersMock.mockResolvedValue([])
    updateUserMock.mockResolvedValue({ ...TEST_USERS[0], disabled: true })
    resetUserPasswordMock.mockResolvedValue(undefined)
    deleteUserMock.mockResolvedValue(undefined)
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders loading skeletons while fetching', () => {
    listUsersMock.mockReturnValue(new Promise(() => {}))

    const { container } = renderPage()

    expect(container.querySelectorAll('.animate-pulse').length).toBeGreaterThan(0)
  })

  it('renders user list after fetch with usernames', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findByText } = renderPage()

    expect(await findByText('alice')).toBeInTheDocument()
    expect(await findByText('bob')).toBeInTheDocument()
  })

  it('formats created_at as YYYY-MM-DD HH:mm via dayjs', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findByText } = renderPage()

    // Compute expected value dynamically so the test is timezone-agnostic
    // (jsdom uses the host's local tz; CI runners vary).
    const expected = dayjs.unix(1_700_000_000).format('YYYY-MM-DD HH:mm')
    expect(await findByText(expected)).toBeInTheDocument()
  })

  it('search input debounces and calls listUsers with q param', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { getByPlaceholderText } = renderPage()

    const input = getByPlaceholderText('Search by username...')
    fireEvent.change(input, { target: { value: 'alice' } })

    await waitFor(() => {
      expect(listUsersMock).toHaveBeenCalledWith({
        q: 'alice',
        page: 1,
        limit: 10,
      })
    })
  })

  it('disable button calls updateUser with { disabled: true }', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText } = renderPage()

    const disableButtons = await findAllByText('Disable')
    fireEvent.click(disableButtons[0])

    await waitFor(() => {
      expect(updateUserMock).toHaveBeenCalledWith('user-1', { disabled: true })
    })
  })

  it('disable success shows a success toast', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText } = renderPage()

    const disableButtons = await findAllByText('Disable')
    fireEvent.click(disableButtons[0])

    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalledWith(
        'User disabled (tokens revoked)',
      )
    })
  })

  it('enable button calls updateUser with { disabled: false } after disable', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText, findByText } = renderPage()

    const disableButtons = await findAllByText('Disable')
    fireEvent.click(disableButtons[0])

    // After disable succeeds the button label flips to "Enable"
    const enableBtn = await findByText('Enable')
    fireEvent.click(enableBtn)

    await waitFor(() => {
      expect(updateUserMock).toHaveBeenCalledWith('user-1', {
        disabled: false,
      })
    })
  })

  it('reset password modal opens showing the target username', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText, findByText } = renderPage()

    const resetButtons = await findAllByText('Reset Password')
    fireEvent.click(resetButtons[0])

    // Modal body uses the phrase "Set a new password for" — unambiguous.
    expect(await findByText(/Set a new password for/)).toBeInTheDocument()
  })

  it('weak password shows inline 422 validation error from backend', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)
    resetUserPasswordMock.mockRejectedValueOnce(
      new AdminApiClientError(
        'VALIDATION',
        'Password must be at least 8 characters with a letter and a digit',
        422,
      ),
    )

    const { findAllByText, findByPlaceholderText, getByText, findByText } =
      renderPage()

    const resetButtons = await findAllByText('Reset Password')
    fireEvent.click(resetButtons[0])

    const input = await findByPlaceholderText('New password')
    fireEvent.change(input, { target: { value: 'abc' } })
    fireEvent.click(getByText('Reset'))

    expect(
      await findByText(
        'Password must be at least 8 characters with a letter and a digit',
      ),
    ).toBeInTheDocument()
  })

  it('strong password calls resetUserPassword and closes modal', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText, findByPlaceholderText, getByText, queryByText } =
      renderPage()

    const resetButtons = await findAllByText('Reset Password')
    fireEvent.click(resetButtons[0])

    const input = await findByPlaceholderText('New password')
    fireEvent.change(input, { target: { value: 'strongpass1' } })
    fireEvent.click(getByText('Reset'))

    await waitFor(() => {
      expect(resetUserPasswordMock).toHaveBeenCalledWith('user-1', {
        new_password: 'strongpass1',
      })
    })
    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalledWith(
        'Password reset successfully',
      )
    })
    // Modal closed → the "Set a new password" copy is gone.
    await waitFor(() => {
      expect(queryByText(/Set a new password for/)).not.toBeInTheDocument()
    })
  })

  it('delete shows confirm dialog before calling deleteUser', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText, findByText } = renderPage()

    const deleteButtons = await findAllByText('Delete')
    fireEvent.click(deleteButtons[0])

    expect(await findByText(/Are you sure/)).toBeInTheDocument()
    expect(deleteUserMock).not.toHaveBeenCalled()
  })

  it('confirming delete calls deleteUser with the user id', async () => {
    listUsersMock.mockResolvedValue(TEST_USERS)

    const { findAllByText, findByRole } = renderPage()

    const deleteButtons = await findAllByText('Delete')
    fireEvent.click(deleteButtons[0])

    const dialog = await findByRole('dialog')
    const confirmBtn = within(dialog).getByText('Delete')
    fireEvent.click(confirmBtn)

    await waitFor(() => {
      expect(deleteUserMock).toHaveBeenCalledWith('user-1')
    })
    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalledWith('User deleted')
    })
  })

  it('shows error message and retry button when fetch fails', async () => {
    listUsersMock.mockRejectedValue(new Error('Network down'))

    const { findByText, getByText } = renderPage()

    expect(await findByText('Network down')).toBeInTheDocument()
    expect(getByText('Retry')).toBeInTheDocument()
  })

  it('shows empty state when no users are returned', async () => {
    listUsersMock.mockResolvedValue([])

    const { findByText } = renderPage()

    expect(await findByText('No users found.')).toBeInTheDocument()
  })
})
