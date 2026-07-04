// Admin Console — route guard for admin-protected pages.
// Clones the user AuthGuard's redirect-to-login behavior but reads
// from adminAuthStore. Token-expiry refresh is left to adminApiClient
// on the next API call; the guard only checks isAuthenticated.
import { Navigate, Outlet } from 'react-router'
import { useAdminAuthStore } from '../../stores/adminAuthStore'

export default function AdminGuard() {
  const isAuthenticated = useAdminAuthStore((s) => s.isAuthenticated)

  if (!isAuthenticated) {
    return <Navigate to="/admin/login" replace />
  }

  return <Outlet />
}
