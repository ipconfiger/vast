import { useMutation } from '@tanstack/react-query'
import { useAuthStore } from '../stores/authStore'

interface UploadResponse {
  file_id: string
  url: string
  original_name: string
  size: number
  mime_type: string
}

export function useUploadFile() {
  const token = useAuthStore((s) => s.token)

  return useMutation({
    mutationFn: async (file: File): Promise<UploadResponse> => {
      const formData = new FormData()
      formData.append('file', file)
      const response = await fetch('/api/files/upload', {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}` },
        body: formData,
      })
      if (!response.ok) {
        const err = await response.json().catch(() => ({}))
        throw new Error(err.error?.message || err.message || 'Upload failed')
      }
      return response.json()
    },
  })
}
