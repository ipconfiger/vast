// Admin Console — isolated auth state.
// Separate persist key ("admin-auth-storage") so admin and user sessions
// never collide in localStorage. Mirrors the shape of authStore but reads
// its own tokens; adminApiClient (api/admin.ts) consumes this store.
import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface AdminTokenPair {
  access_token: string
  refresh_token: string
  expires_in: number
}

interface AdminAuthState {
  adminToken: string | null
  adminRefreshToken: string | null
  adminTokenExpiry: number | null // Unix ms when the access token expires
  isAuthenticated: boolean
  username: string | null
  login: (tokens: AdminTokenPair, username: string) => void
  logout: () => void
  setTokens: (tokens: AdminTokenPair) => void
  isTokenExpired: () => boolean
}

function computeExpiry(expires_in: number): number {
  return Date.now() + expires_in * 1000
}

export const useAdminAuthStore = create<AdminAuthState>()(
  persist(
    (set, get) => ({
      adminToken: null,
      adminRefreshToken: null,
      adminTokenExpiry: null,
      isAuthenticated: false,
      username: null,
      login: (tokens, username) =>
        set({
          adminToken: tokens.access_token,
          adminRefreshToken: tokens.refresh_token,
          adminTokenExpiry: computeExpiry(tokens.expires_in),
          isAuthenticated: true,
          username,
        }),
      logout: () =>
        set({
          adminToken: null,
          adminRefreshToken: null,
          adminTokenExpiry: null,
          isAuthenticated: false,
          username: null,
        }),
      setTokens: (tokens) =>
        set({
          adminToken: tokens.access_token,
          adminRefreshToken: tokens.refresh_token,
          adminTokenExpiry: computeExpiry(tokens.expires_in),
          isAuthenticated: true,
        }),
      isTokenExpired: () => {
        const { adminTokenExpiry } = get()
        if (!adminTokenExpiry) return true
        return Date.now() >= adminTokenExpiry - 30_000 // 30s buffer
      },
    }),
    {
      name: 'admin-auth-storage',
      // Persist only data fields; isAuthenticated is derived on rehydrate.
      partialize: (state) => ({
        adminToken: state.adminToken,
        adminRefreshToken: state.adminRefreshToken,
        adminTokenExpiry: state.adminTokenExpiry,
        username: state.username,
      }),
      onRehydrateStorage: () => (state) => {
        if (state && state.adminToken) {
          state.isAuthenticated = true
        }
      },
    },
  ),
)
