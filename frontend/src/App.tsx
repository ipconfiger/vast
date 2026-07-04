import { QueryClientProvider } from '@tanstack/react-query'
import { queryClient } from './queryClient'
import { createBrowserRouter, RouterProvider, Outlet } from 'react-router'
import { ChannelListPage } from './pages/ChannelListPage'
import { RequestsPage } from './pages/RequestsPage'
import { SearchPage } from './pages/SearchPage'
import { DirectMessagePage } from './pages/DirectMessagePage'
import ProfilePage from './pages/ProfilePage'
import { ThreadView } from './pages/ThreadView'
import LoginPage from './pages/LoginPage'
import RegisterPage from './pages/RegisterPage'
import AuthGuard, { PublicRouteGuard } from './components/AuthGuard'
import { ErrorBoundary } from './components/ErrorBoundary'
import { ToastContainer } from './components/ToastContainer'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
import { useWebSocket } from './hooks/useWebSocket'
import AdminGuard from './components/admin/AdminGuard'
import AdminPublicRouteGuard from './components/admin/AdminPublicRouteGuard'
import AdminLayout from './components/admin/AdminLayout'
import AdminLoginPage from './pages/admin/AdminLoginPage'
import AdminDashboardPage from './pages/admin/AdminDashboardPage'

function AppLayout() {
  useKeyboardShortcuts()
  useWebSocket()
  return (
    <>
      <Outlet />
      <ToastContainer />
    </>
  )
}

const router = createBrowserRouter([
  {
    element: (
      <ErrorBoundary>
        <AppLayout />
      </ErrorBoundary>
    ),
    children: [
      {
        path: '/login',
        element: (
          <PublicRouteGuard>
            <LoginPage />
          </PublicRouteGuard>
        ),
      },
      {
        path: '/register',
        element: (
          <PublicRouteGuard>
            <RegisterPage />
          </PublicRouteGuard>
        ),
      },
      {
        element: <AuthGuard />,
        children: [
          {
            path: '/channels',
            element: <ChannelListPage />,
          },
          {
            path: '/channels/:channelId',
            element: <ChannelListPage />,
          },
          {
            path: '/channels/:channelId/requests',
            element: <RequestsPage />,
          },
          {
            path: '/channels/:channelId/thread/:messageId',
            element: <ThreadView />,
          },
          {
            path: '/dm/:userId',
            element: <DirectMessagePage />,
          },
          {
            path: '/profile',
            element: <ProfilePage />,
          },
          {
            path: '/search',
            element: <SearchPage />,
          },
          {
            path: '/',
            element: <ChannelListPage />,
          },
        ],
      },
    ],
  },
  // Admin Console — top-level sibling, NOT under AppLayout (admin has no
  // WebSocket session or user keyboard shortcuts). AdminGuard renders
  // <Outlet/> so the layout is a separate pathless route nesting level.
  {
    path: '/admin/login',
    element: (
      <AdminPublicRouteGuard>
        <AdminLoginPage />
      </AdminPublicRouteGuard>
    ),
  },
  {
    path: '/admin',
    element: <AdminGuard />,
    children: [
      {
        element: <AdminLayout />,
        children: [
          {
            index: true,
            element: <AdminDashboardPage />,
          },
          {
            path: 'users',
            element: (
              <div className="p-8 text-zinc-400">
                User management — coming in T11
              </div>
            ),
          },
          {
            path: 'invite-codes',
            element: (
              <div className="p-8 text-zinc-400">
                Invite codes — coming in T12
              </div>
            ),
          },
          {
            path: 'audit-logs',
            element: (
              <div className="p-8 text-zinc-400">
                Audit logs — coming in T13
              </div>
            ),
          },
        ],
      },
    ],
  },
])

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}

export default App
