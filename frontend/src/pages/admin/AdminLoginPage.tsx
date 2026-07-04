// Admin Console — login page.
// Visual design matches LoginPage.tsx (dark slate card, indigo
// accent). Form uses username/password — admins are configured via
// ADMIN_USERNAME / ADMIN_PASSWORD env vars on the backend.
import { useState, type FormEvent } from 'react'
import { useMutation } from '@tanstack/react-query'
import { useNavigate, Link } from 'react-router'
import { Lock, User, Shield, ArrowLeft } from 'lucide-react'
import { useAdminAuthStore } from '../../stores/adminAuthStore'
import { adminLogin } from '../../api/admin'

interface AdminLoginError {
  message?: string
}

export default function AdminLoginPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const storeLogin = useAdminAuthStore((s) => s.login)
  const navigate = useNavigate()

  const mutation = useMutation<
    Awaited<ReturnType<typeof adminLogin>>,
    AdminLoginError,
    { username: string; password: string }
  >({
    mutationFn: async (data) => {
      // adminLogin calls fetch directly (not adminApiClient) so a 401
      // ("bad credentials") surfaces as mutation.error instead of
      // triggering the client's logout side-effect mid-mutation.
      try {
        return await adminLogin(data.username, data.password)
      } catch (e) {
        // AdminApiClientError extends Error; network errors are Error too.
        const message =
          e instanceof Error ? e.message : 'Login failed. Please try again.'
        throw { message } as AdminLoginError
      }
    },
    onSuccess: (data) => {
      storeLogin(data, username)
      navigate('/admin')
    },
  })

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    mutation.mutate({ username, password })
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-950 px-4">
      <div className="w-full max-w-md">
        <div className="bg-slate-900 rounded-2xl shadow-2xl border border-slate-800 p-8">
          <div className="text-center mb-8">
            <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-indigo-600/20 border border-indigo-500/30 mb-4">
              <Shield className="w-8 h-8 text-indigo-400" />
            </div>
            <h1 className="text-2xl font-bold text-white tracking-tight">
              Admin Console
            </h1>
            <p className="text-slate-400 mt-1.5 text-sm">
              Sign in with administrator credentials
            </p>
          </div>

          {mutation.isError && (
            <div className="mb-6 p-3 rounded-lg bg-red-500/10 border border-red-500/30">
              <p className="text-red-400 text-sm">
                {mutation.error?.message || 'Login failed. Please try again.'}
              </p>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label
                htmlFor="admin-username"
                className="block text-sm font-medium text-slate-300 mb-1.5"
              >
                Username
              </label>
              <div className="relative">
                <User className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  id="admin-username"
                  type="text"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  required
                  autoComplete="username"
                  placeholder="Enter admin username"
                  className="w-full pl-10 pr-4 py-2.5 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500/50 transition-all"
                />
              </div>
            </div>

            <div>
              <label
                htmlFor="admin-password"
                className="block text-sm font-medium text-slate-300 mb-1.5"
              >
                Password
              </label>
              <div className="relative">
                <Lock className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  id="admin-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  autoComplete="current-password"
                  placeholder="Enter admin password"
                  className="w-full pl-10 pr-4 py-2.5 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500/50 transition-all"
                />
              </div>
            </div>

            <button
              type="submit"
              disabled={mutation.isPending}
              className="w-full py-2.5 px-4 bg-indigo-600 hover:bg-indigo-500 disabled:bg-indigo-600/50 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-500/50 flex items-center justify-center gap-2"
            >
              {mutation.isPending ? (
                <>
                  <svg
                    className="animate-spin h-4 w-4"
                    xmlns="http://www.w3.org/2000/svg"
                    fill="none"
                    viewBox="0 0 24 24"
                    aria-hidden="true"
                  >
                    <circle
                      className="opacity-25"
                      cx="12"
                      cy="12"
                      r="10"
                      stroke="currentColor"
                      strokeWidth="4"
                    />
                    <path
                      className="opacity-75"
                      fill="currentColor"
                      d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                    />
                  </svg>
                  Signing in...
                </>
              ) : (
                'Sign in'
              )}
            </button>
          </form>

          <p className="mt-6 text-center text-sm text-slate-400">
            <Link
              to="/"
              className="inline-flex items-center gap-1.5 text-slate-400 hover:text-slate-300 font-medium transition-colors"
            >
              <ArrowLeft className="w-3.5 h-3.5" />
              Back to app
            </Link>
          </p>
        </div>
      </div>
    </div>
  )
}
