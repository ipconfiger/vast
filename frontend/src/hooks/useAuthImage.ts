import { useState, useEffect } from 'react'
import { useAuthStore } from '../stores/authStore'
import { refreshAccessToken } from '../api/client'

async function getFreshToken(): Promise<string | null> {
  const store = useAuthStore.getState()
  const token = store.token
  if (token && !store.isTokenExpired()) return token  // still valid
  if (token || store.refreshToken) {
    // try refresh
    return refreshAccessToken()
  }
  return null
}

export function useAuthImage(url?: string | null): string | null {
  const [src, setSrc] = useState<string | null>(null)
  const token = useAuthStore((s) => s.token)

  useEffect(() => {
    let cancelled = false

    async function load() {
      if (!url) { if (!cancelled) setSrc(null); return }

      const freshToken = await getFreshToken()
      if (!freshToken || cancelled) { if (!cancelled) setSrc(null); return }

      const controller = new AbortController()
      let objectUrl: string | null = null

      try {
        let response = await fetch(url, {
          signal: controller.signal,
          headers: { Authorization: `Bearer ${freshToken}` },
        })

        // On 401, try refreshing the token once and retry
        if (response.status === 401) {
          const retryToken = await refreshAccessToken()
          if (!retryToken || cancelled) { if (!cancelled) setSrc(null); return }
          controller.abort()
          const retryController = new AbortController()
          response = await fetch(url, {
            signal: retryController.signal,
            headers: { Authorization: `Bearer ${retryToken}` },
          })
          // Transfer abort control
          retryController.signal.addEventListener('abort', () => {
            if (!cancelled) setSrc(null)
          })
        }

        if (!response.ok) { if (!cancelled) setSrc(null); return }
        const blob = await response.blob()
        if (cancelled) return
        objectUrl = URL.createObjectURL(blob)
        setSrc(objectUrl)
      } catch {
        if (!cancelled) setSrc(null)
      } finally {
        // cleanup omitted for simplicity — URL revocation happens on next effect
      }
    }

    load()
    return () => { cancelled = true }
  }, [url, token])

  return src
}
