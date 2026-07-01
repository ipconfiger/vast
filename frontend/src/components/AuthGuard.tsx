import { useEffect, useState, type ReactNode } from 'react'
import { Navigate, Outlet } from 'react-router'
import { useAuthStore } from '../stores/authStore'
import { refreshAccessToken } from '../api/client'
import { Loader2 } from 'lucide-react'

export { refreshAccessToken }

function AuthLoadingScreen() {
  return (
    <div className="flex h-screen items-center justify-center bg-zinc-950">
      <div className="flex flex-col items-center gap-3">
        <Loader2 className="h-8 w-8 animate-spin text-zinc-500" />
        <p className="text-sm text-zinc-500">Verifying session...</p>
      </div>
    </div>
  )
}

interface AuthGuardProps {
  children?: ReactNode
}

export default function AuthGuard({ children }: AuthGuardProps) {
  const token = useAuthStore((s) => s.token)
  const isTokenExpired = useAuthStore((s) => s.isTokenExpired)
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const logout = useAuthStore((s) => s.logout)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const [refreshFailed, setRefreshFailed] = useState(false)

  useEffect(() => {
    let cancelled = false

    async function checkAndRefresh() {
      if (!token || !isAuthenticated) return

      if (!isTokenExpired()) return

      setIsRefreshing(true)
      try {
        const newToken = await refreshAccessToken()
        if (!cancelled) {
          if (!newToken) {
            setRefreshFailed(true)
            logout()
          } else {
            setIsRefreshing(false)
          }
        }
      } catch {
        if (!cancelled) {
          setRefreshFailed(true)
          logout()
        }
      }
    }

    checkAndRefresh()

    return () => {
      cancelled = true
    }
  }, [token, isAuthenticated, isTokenExpired, logout])

  // If no token at all, redirect to login
  if (!token && !isAuthenticated) {
    return <Navigate to="/login" replace />
  }

  // If refreshing, show loading
  if (isRefreshing) {
    return <AuthLoadingScreen />
  }

  // If refresh failed, redirect to login
  if (refreshFailed) {
    return <Navigate to="/login" replace />
  }

  // Token is valid (either fresh or successfully refreshed)
  return children ? <>{children}</> : <Outlet />
}

/**
 * Guard for public routes (login, register).
 * Redirects to /channels if user is already authenticated with a valid token.
 */
export function PublicRouteGuard({ children }: { children: ReactNode }) {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const token = useAuthStore((s) => s.token)
  const isTokenExpired = useAuthStore((s) => s.isTokenExpired)

  if (isAuthenticated && token && !isTokenExpired()) {
    return <Navigate to="/channels" replace />
  }

  return <>{children}</>
}
