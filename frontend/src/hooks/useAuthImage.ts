import { useState, useEffect } from 'react'
import { useAuthStore } from '../stores/authStore'

export function useAuthImage(url?: string | null): string | null {
  const [src, setSrc] = useState<string | null>(null)

  useEffect(() => {
    if (!url) { setSrc(null); return }
    const token = useAuthStore.getState().token
    if (!token) { setSrc(null); return }
    let cancelled = false
    fetch(url, { headers: { Authorization: `Bearer ${token}` } })
      .then(r => r.blob())
      .then(blob => {
        if (!cancelled) setSrc(URL.createObjectURL(blob))
      })
      .catch(() => { if (!cancelled) setSrc(null) })
    return () => { cancelled = true }
  }, [url])

  return src
}
