import { useMutation } from '@tanstack/react-query'
import { apiClient } from './client'

interface UploadResponse {
  file_id: string
  url: string
  original_name: string
  size: number
  mime_type: string
}

export type { UploadResponse }

export function useUploadFile() {
  return useMutation({
    mutationFn: async (file: File): Promise<UploadResponse> => {
      const formData = new FormData()
      formData.append('file', file)
      // apiClient injects the Bearer token, prepends API_BASE, and throws
      // ApiClientError on non-2xx (it does the res.ok check internally).
      // FormData bodies skip the json Content-Type so the browser can set
      // the multipart boundary (T5).
      return apiClient<UploadResponse>('/files/upload', {
        method: 'POST',
        body: formData,
      })
    },
  })
}
