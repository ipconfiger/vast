import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router'
import { createElement, type ReactNode } from 'react'
import AdminLoginPage from './AdminLoginPage'
import { useAdminAuthStore } from '../../stores/adminAuthStore'

// --- Mocks -----------------------------------------------------------------

const adminLoginMock = vi.fn()
vi.mock('../../api/admin', () => ({
  adminLogin: (...args: unknown[]) => adminLoginMock(...args),
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

const mockNavigate = vi.fn()
vi.mock('react-router', () => ({
  useNavigate: () => mockNavigate,
  MemoryRouter: ({ children }: { children: ReactNode }) => children,
  Link: ({ children, to }: { children: ReactNode; to: string }) =>
    createElement('a', { href: to }, children),
}))

// --- Helpers ---------------------------------------------------------------

function resetStore() {
  localStorage.removeItem('admin-auth-storage')
  useAdminAuthStore.setState({
    adminToken: null,
    adminRefreshToken: null,
    adminTokenExpiry: null,
    isAuthenticated: false,
    username: null,
  })
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{createElement(AdminLoginPage)}</MemoryRouter>
    </QueryClientProvider>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminLoginPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetStore()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders username and password inputs with admin branding', () => {
    const { container } = renderPage()

    const usernameInput = container.querySelector('#admin-username')
    const passwordInput = container.querySelector('#admin-password')

    expect(usernameInput).toBeTruthy()
    expect(usernameInput?.getAttribute('type')).toBe('text')
    expect(passwordInput).toBeTruthy()
    expect(passwordInput?.getAttribute('type')).toBe('password')

    // Admin branding distinguishes from user login
    expect(container.textContent).toContain('Admin Console')
  })

  it('calls adminLogin and navigates to /admin on success', async () => {
    adminLoginMock.mockResolvedValueOnce({
      access_token: 'admin-access',
      refresh_token: 'admin-refresh',
      expires_in: 3600,
    })

    const { container } = renderPage()

    const usernameInput = container.querySelector(
      '#admin-username',
    ) as HTMLInputElement
    const passwordInput = container.querySelector(
      '#admin-password',
    ) as HTMLInputElement
    const form = container.querySelector('form') as HTMLFormElement

    fireEvent.change(usernameInput, { target: { value: 'admin' } })
    fireEvent.change(passwordInput, { target: { value: 'secret123' } })
    fireEvent.submit(form)

    await waitFor(() => {
      expect(adminLoginMock).toHaveBeenCalledTimes(1)
    })

    expect(adminLoginMock).toHaveBeenCalledWith('admin', 'secret123')

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith('/admin')
    })

    // Store is populated with tokens + username
    const s = useAdminAuthStore.getState()
    expect(s.adminToken).toBe('admin-access')
    expect(s.adminRefreshToken).toBe('admin-refresh')
    expect(s.username).toBe('admin')
    expect(s.isAuthenticated).toBe(true)
  })

  it('shows an error message on 401 (wrong credentials)', async () => {
    // adminLogin throws AdminApiClientError (extends Error) with the
    // backend's message; the page reads e.message.
    adminLoginMock.mockRejectedValueOnce(new Error('Invalid credentials'))

    const { container } = renderPage()

    const usernameInput = container.querySelector(
      '#admin-username',
    ) as HTMLInputElement
    const passwordInput = container.querySelector(
      '#admin-password',
    ) as HTMLInputElement
    const form = container.querySelector('form') as HTMLFormElement

    fireEvent.change(usernameInput, { target: { value: 'admin' } })
    fireEvent.change(passwordInput, { target: { value: 'wrong' } })
    fireEvent.submit(form)

    await waitFor(() => {
      expect(container.textContent).toContain('Invalid credentials')
    })

    // Must NOT navigate on failure
    expect(mockNavigate).not.toHaveBeenCalled()
    // Store must remain unauthenticated
    expect(useAdminAuthStore.getState().isAuthenticated).toBe(false)
  })
})
