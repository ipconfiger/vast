import { useState, useEffect } from 'react'
import { useAuthStore } from '../stores/authStore'

export function useAuthImage(url?: string | null): string | null {
  const [src, setSrc] = useState<string | null>(null)

  useEffect(() => {
    if (!url) { setSrc(null); return }
    const token = useAuthStore.getState().token
    if (!token) { setSrc(null); return }
    const controller = new AbortController()
    let objectUrl: string | null = null
    fetch(url, { signal: controller.signal, headers: { Authorization: `Bearer ${token}` } })
      .then(r => r.blob())
      .then(blob => {
        objectUrl = URL.createObjectURL(blob)
        setSrc(objectUrl)
      })
      .catch(() => { setSrc(null) })
    return () => {
      if (objectUrl) URL.revokeObjectURL(objectUrl)
      controller.abort()
    }
  }, [url])

  return src
}
