import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createBrowserRouter, RouterProvider, Outlet } from 'react-router'
import { ChannelListPage } from './pages/ChannelListPage'
import { RequestsPage } from './pages/RequestsPage'
import LoginPage from './pages/LoginPage'
import RegisterPage from './pages/RegisterPage'
import AuthGuard, { PublicRouteGuard } from './components/AuthGuard'
import { ErrorBoundary } from './components/ErrorBoundary'
import { ToastContainer } from './components/ToastContainer'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 30_000,
      refetchOnWindowFocus: false,
    },
  },
})

function AppLayout() {
  useKeyboardShortcuts()
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
            element: <div>Thread View</div>,
          },
          {
            path: '/dm/:userId',
            element: <div>Direct Message</div>,
          },
          {
            path: '/search',
            element: <div>Search Page</div>,
          },
          {
            path: '/',
            element: <ChannelListPage />,
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
