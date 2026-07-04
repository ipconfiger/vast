import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent, waitFor } from '@testing-library/react'
import { MemoryRouter, Routes, Route } from 'react-router'
import { createElement } from 'react'
import AdminLayout from './AdminLayout'
import { useAdminAuthStore } from '../../stores/adminAuthStore'

// --- Mocks -----------------------------------------------------------------

// adminLogout is the proper logout flow (backend + local state). Stub it
// so the test doesn't hit the network; verify it's called on logout click.
const adminLogoutMock = vi.fn()
vi.mock('../../api/admin', () => ({
  adminLogout: (...args: unknown[]) => adminLogoutMock(...args),
}))

// Partial mock: keep NavLink/Outlet/MemoryRouter/Routes real, stub useNavigate
// so we can assert the post-logout redirect target.
const mockNavigate = vi.fn()
vi.mock('react-router', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-router')>()
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  }
})

// --- Helpers ---------------------------------------------------------------

function resetStore() {
  localStorage.removeItem('admin-auth-storage')
  useAdminAuthStore.setState({
    adminToken: 'test-admin-token',
    adminRefreshToken: 'test-admin-refresh',
    adminTokenExpiry: Date.now() + 3_600_000,
    isAuthenticated: true,
    username: 'admin-user',
  })
}

function renderLayout(route = '/admin') {
  return render(
    <MemoryRouter initialEntries={[route]}>
      <Routes>
        <Route path="/admin" element={createElement(AdminLayout)}>
          <Route index element={createElement('div', null, 'dashboard content')}
          />
          <Route path="users" element={createElement('div', null, 'users content')} />
        </Route>
      </Routes>
    </MemoryRouter>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminLayout', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetStore()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders all 4 sidebar nav links with correct labels', () => {
    const { container } = renderLayout()

    expect(container.textContent).toContain('Dashboard')
    expect(container.textContent).toContain('Users')
    expect(container.textContent).toContain('Invite Codes')
    expect(container.textContent).toContain('Audit Logs')
  })

  it('shows admin branding and username in the topbar', () => {
    const { container } = renderLayout()

    expect(container.textContent).toContain('Admin Console')
    expect(container.textContent).toContain('admin-user')
    expect(container.textContent).toContain('Admin')
  })

  it('renders Outlet content for nested routes', () => {
    const { container } = renderLayout()

    expect(container.textContent).toContain('dashboard content')
  })

  it('logout button calls adminLogout and navigates to /admin/login', async () => {
    adminLogoutMock.mockResolvedValue(undefined)

    const { getByText } = renderLayout()
    const logoutBtn = getByText('Logout')

    fireEvent.click(logoutBtn)

    await waitFor(() => {
      expect(adminLogoutMock).toHaveBeenCalledTimes(1)
    })
    expect(mockNavigate).toHaveBeenCalledWith('/admin/login')
  })
})
