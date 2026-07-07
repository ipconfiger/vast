import { useQuery, useMutation, useInfiniteQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from './client'
import type { FileRecord, FileListResponse, FileFilters } from '../types'

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

export function useFiles(filters: FileFilters = {}) {
  return useQuery({
    queryKey: ['files', filters],
    queryFn: async (): Promise<FileListResponse> => {
      const params = new URLSearchParams()
      if (filters.channel_id) params.set('channel_id', filters.channel_id)
      if (filters.uploader_id) params.set('uploader_id', filters.uploader_id)
      if (filters.mime_type) params.set('mime_type', filters.mime_type)
      if (filters.mime_prefix) params.set('mime_prefix', filters.mime_prefix)
      if (filters.size_min !== undefined && filters.size_min > 0) params.set('size_min', String(filters.size_min))
      if (filters.size_max !== undefined && filters.size_max > 0) params.set('size_max', String(filters.size_max))
      if (filters.created_after) params.set('created_after', String(filters.created_after))
      if (filters.created_before) params.set('created_before', String(filters.created_before))
      if (filters.search) params.set('search', filters.search)
      if (filters.sort_by && filters.sort_by !== 'created_at') params.set('sort_by', filters.sort_by)
      if (filters.sort_order && filters.sort_order !== 'desc') params.set('sort_order', filters.sort_order)
      if (filters.cursor) params.set('cursor', filters.cursor)
      if (filters.limit) params.set('limit', String(filters.limit))

      const qs = params.toString()
      return apiClient<FileListResponse>(`/files${qs ? '?' + qs : ''}`)
    },
  })
}

export function useInfiniteFiles(filters: Omit<FileFilters, 'cursor'> = {}) {
  return useInfiniteQuery({
    queryKey: ['files', 'infinite', filters],
    queryFn: async ({ pageParam }): Promise<FileListResponse> => {
      const params = new URLSearchParams()
      if (filters.channel_id) params.set('channel_id', filters.channel_id)
      if (filters.uploader_id) params.set('uploader_id', filters.uploader_id)
      if (filters.mime_type) params.set('mime_type', filters.mime_type)
      if (filters.mime_prefix) params.set('mime_prefix', filters.mime_prefix)
      if (filters.size_min !== undefined && filters.size_min > 0) params.set('size_min', String(filters.size_min))
      if (filters.size_max !== undefined && filters.size_max > 0) params.set('size_max', String(filters.size_max))
      if (filters.created_after) params.set('created_after', String(filters.created_after))
      if (filters.created_before) params.set('created_before', String(filters.created_before))
      if (filters.search) params.set('search', filters.search)
      if (filters.sort_by && filters.sort_by !== 'created_at') params.set('sort_by', filters.sort_by)
      if (filters.sort_order && filters.sort_order !== 'desc') params.set('sort_order', filters.sort_order)
      if (pageParam) params.set('cursor', pageParam)

      const qs = params.toString()
      return apiClient<FileListResponse>(`/files${qs ? '?' + qs : ''}`)
    },
    initialPageParam: '',
    getNextPageParam: (lastPage) => lastPage.has_more ? lastPage.next_cursor : undefined,
  })
}

export function useDeleteFile() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (fileId: string): Promise<void> => {
      await apiClient(`/files/${fileId}`, { method: 'DELETE' })
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files'] })
    },
  })
}

export type { FileRecord, FileListResponse, FileFilters }
