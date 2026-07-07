import { QueryClientProvider } from '@tanstack/react-query'
import { queryClient } from './queryClient'
import { createBrowserRouter, RouterProvider, Outlet } from 'react-router'
import { ChannelListPage } from './pages/ChannelListPage'
import { RequestsPage } from './pages/RequestsPage'
import { SearchPage } from './pages/SearchPage'
import { FilesPage } from './pages/FilesPage'
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
import { useUnreadTracker } from './hooks/useUnreadTracker'
import AdminGuard from './components/admin/AdminGuard'
import AdminPublicRouteGuard from './components/admin/AdminPublicRouteGuard'
import AdminLayout from './components/admin/AdminLayout'
import AdminLoginPage from './pages/admin/AdminLoginPage'
import AdminDashboardPage from './pages/admin/AdminDashboardPage'
import AdminUsersPage from './pages/admin/AdminUsersPage'
import AdminInviteCodesPage from './pages/admin/AdminInviteCodesPage'
import AdminAuditLogsPage from './pages/admin/AdminAuditLogsPage'
import AdminBotsPage from './pages/admin/AdminBotsPage'

function AppLayout() {
  useKeyboardShortcuts()
  useWebSocket()
  useUnreadTracker()
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
            path: '/files',
            element: <FilesPage />,
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
            element: <AdminUsersPage />,
          },
          {
            path: 'invite-codes',
            element: <AdminInviteCodesPage />,
          },
          {
            path: 'bots',
            element: <AdminBotsPage />,
          },
          {
            path: 'audit-logs',
            element: <AdminAuditLogsPage />,
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
