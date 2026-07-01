import { useState, type FormEvent } from 'react'
import { useMutation } from '@tanstack/react-query'
import { useNavigate, Link } from 'react-router'
import { Lock, User, KeyRound } from 'lucide-react'
import { apiClient } from '../api/client'
import { useAuthStore } from '../stores/authStore'
import type { User as UserType } from '../types'

interface RegisterResponse {
  access_token: string
  refresh_token: string
  user: UserType
}

interface RegisterError {
  message?: string
  errors?: Record<string, string>
}

interface FieldErrors {
  username?: string
  password?: string
  inviteCode?: string
}

export default function RegisterPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [inviteCode, setInviteCode] = useState('')
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({})
  const storeLogin = useAuthStore((s) => s.login)
  const navigate = useNavigate()

  const mutation = useMutation<
    RegisterResponse,
    RegisterError,
    { username: string; password: string; invite_code: string }
  >({
    mutationFn: (data) =>
      apiClient<RegisterResponse>('/auth/register', {
        method: 'POST',
        body: JSON.stringify(data),
      }),
    onSuccess: (data) => {
      storeLogin(
        { access_token: data.access_token, refresh_token: data.refresh_token },
        data.user,
      )
      navigate('/channels')
    },
    onError: (error) => {
      if (error?.errors) {
        setFieldErrors({
          username: error.errors.username,
          password: error.errors.password,
          inviteCode: error.errors.invite_code,
        })
      }
    },
  })

  const validate = (): boolean => {
    const errors: FieldErrors = {}
    if (!username.trim()) {
      errors.username = 'Username is required'
    } else if (username.trim().length < 3) {
      errors.username = 'Username must be at least 3 characters'
    }
    if (!password) {
      errors.password = 'Password is required'
    } else if (password.length < 6) {
      errors.password = 'Password must be at least 6 characters'
    }
    if (!inviteCode.trim()) {
      errors.inviteCode = 'Invite code is required'
    }
    setFieldErrors(errors)
    return Object.keys(errors).length === 0
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!validate()) return
    mutation.mutate({
      username: username.trim(),
      password,
      invite_code: inviteCode.trim(),
    })
  }

  const inputClass = (field: keyof FieldErrors) =>
    `w-full pl-10 pr-4 py-2.5 bg-slate-800 border rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 transition-all ${
      fieldErrors[field]
        ? 'border-red-500/60 focus:ring-red-500/50 focus:border-red-500/50'
        : 'border-slate-700 focus:ring-indigo-500/50 focus:border-indigo-500/50'
    }`

  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-950 px-4">
      <div className="w-full max-w-md">
        <div className="bg-slate-900 rounded-2xl shadow-2xl border border-slate-800 p-8">
          <div className="text-center mb-8">
            <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-indigo-600/20 border border-indigo-500/30 mb-4">
              <KeyRound className="w-8 h-8 text-indigo-400" />
            </div>
            <h1 className="text-2xl font-bold text-white tracking-tight">
              Create account
            </h1>
            <p className="text-slate-400 mt-1.5 text-sm">
              Join the conversation
            </p>
          </div>

          {mutation.isError && mutation.error?.message && !mutation.error?.errors && (
            <div className="mb-6 p-3 rounded-lg bg-red-500/10 border border-red-500/30">
              <p className="text-red-400 text-sm">
                {mutation.error.message}
              </p>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label
                htmlFor="reg-username"
                className="block text-sm font-medium text-slate-300 mb-1.5"
              >
                Username
              </label>
              <div className="relative">
                <User className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  id="reg-username"
                  type="text"
                  value={username}
                  onChange={(e) => {
                    setUsername(e.target.value)
                    if (fieldErrors.username) {
                      setFieldErrors((prev) => ({ ...prev, username: undefined }))
                    }
                  }}
                  required
                  autoComplete="username"
                  placeholder="Choose a username"
                  className={inputClass('username')}
                />
              </div>
              {fieldErrors.username && (
                <p className="mt-1.5 text-sm text-red-400">{fieldErrors.username}</p>
              )}
            </div>

            <div>
              <label
                htmlFor="reg-password"
                className="block text-sm font-medium text-slate-300 mb-1.5"
              >
                Password
              </label>
              <div className="relative">
                <Lock className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  id="reg-password"
                  type="password"
                  value={password}
                  onChange={(e) => {
                    setPassword(e.target.value)
                    if (fieldErrors.password) {
                      setFieldErrors((prev) => ({ ...prev, password: undefined }))
                    }
                  }}
                  required
                  autoComplete="new-password"
                  placeholder="Create a password"
                  className={inputClass('password')}
                />
              </div>
              {fieldErrors.password && (
                <p className="mt-1.5 text-sm text-red-400">{fieldErrors.password}</p>
              )}
            </div>

            <div>
              <label
                htmlFor="reg-invite-code"
                className="block text-sm font-medium text-slate-300 mb-1.5"
              >
                Invite Code
              </label>
              <div className="relative">
                <KeyRound className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  id="reg-invite-code"
                  type="text"
                  value={inviteCode}
                  onChange={(e) => {
                    setInviteCode(e.target.value)
                    if (fieldErrors.inviteCode) {
                      setFieldErrors((prev) => ({ ...prev, inviteCode: undefined }))
                    }
                  }}
                  required
                  placeholder="Enter your invite code"
                  className={inputClass('inviteCode')}
                />
              </div>
              {fieldErrors.inviteCode && (
                <p className="mt-1.5 text-sm text-red-400">{fieldErrors.inviteCode}</p>
              )}
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
                  Creating account...
                </>
              ) : (
                'Create account'
              )}
            </button>
          </form>

          <p className="mt-6 text-center text-sm text-slate-400">
            Already have an account?{' '}
            <Link
              to="/login"
              className="text-indigo-400 hover:text-indigo-300 font-medium transition-colors"
            >
              Sign in
            </Link>
          </p>
        </div>
      </div>
    </div>
  )
}
