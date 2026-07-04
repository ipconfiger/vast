// Admin Console — public route guard for /admin/login.
// Redirects already-authenticated admins to /admin so the login
// page is not shown to logged-in users. Mirrors the user
// PublicRouteGuard pattern.
import { type ReactNode } from 'react'
import { Navigate } from 'react-router'
import { useAdminAuthStore } from '../../stores/adminAuthStore'

export default function AdminPublicRouteGuard({
  children,
}: {
  children: ReactNode
}) {
  const isAuthenticated = useAdminAuthStore((s) => s.isAuthenticated)
  const adminToken = useAdminAuthStore((s) => s.adminToken)
  const isTokenExpired = useAdminAuthStore((s) => s.isTokenExpired)

  if (isAuthenticated && adminToken && !isTokenExpired()) {
    return <Navigate to="/admin" replace />
  }

  return <>{children}</>
}
