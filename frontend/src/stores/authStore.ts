import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { User } from '../types'

interface TokenPair {
  access_token: string
  refresh_token: string
}

interface AuthState {
  token: string | null
  refreshToken: string | null
  tokenExpiry: number | null
  user: User | null
  isAuthenticated: boolean
  setTokens: (tokens: TokenPair) => void
  login: (tokens: TokenPair, user: User) => void
  logout: () => void
  register: (tokens: TokenPair, user: User) => void
  setUser: (user: User) => void
  isTokenExpired: () => boolean
}

function decodeJwtExpiry(token: string): number | null {
  try {
    const [, payloadB64] = token.split('.')
    const payload = JSON.parse(atob(payloadB64))
    return payload.exp ? payload.exp * 1000 : null
  } catch {
    return null
  }
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      refreshToken: null,
      tokenExpiry: null,
      user: null,
      isAuthenticated: false,
      setTokens: (tokens) =>
        set({
          token: tokens.access_token,
          refreshToken: tokens.refresh_token,
          tokenExpiry: decodeJwtExpiry(tokens.access_token),
          isAuthenticated: true,
        }),
      login: (tokens, user) =>
        set({
          token: tokens.access_token,
          refreshToken: tokens.refresh_token,
          tokenExpiry: decodeJwtExpiry(tokens.access_token),
          user,
          isAuthenticated: true,
        }),
      logout: () => {
        localStorage.removeItem('auth-storage')
        set({
          token: null,
          refreshToken: null,
          tokenExpiry: null,
          user: null,
          isAuthenticated: false,
        })
      },
      register: (tokens, user) =>
        set({
          token: tokens.access_token,
          refreshToken: tokens.refresh_token,
          tokenExpiry: decodeJwtExpiry(tokens.access_token),
          user,
          isAuthenticated: true,
        }),
      setUser: (user) => set({ user }),
      isTokenExpired: () => {
        const { tokenExpiry } = get()
        if (!tokenExpiry) return true
        return Date.now() >= tokenExpiry - 30_000 // 30s buffer
      },
    }),
    {
      name: 'auth-storage',
    },
  ),
)
