import { useState, useCallback, useRef, useEffect } from 'react'
import { useNavigate } from 'react-router'
import { ArrowLeft, Loader2, FolderOpen } from 'lucide-react'
import { useInfiniteFiles, useDeleteFile } from '../api/files'
import { useAuthStore } from '../stores/authStore'
import { FileFilterBar } from '../components/FileFilterBar'
import { FileCard } from '../components/FileCard'
import type { FileFilters } from '../types'

const DEFAULT_FILTERS: FileFilters = {}

export function FilesPage() {
  const navigate = useNavigate()
  const currentUserId = useAuthStore((s) => s.user?.id ?? '')
  const [filters, setFilters] = useState<FileFilters>(DEFAULT_FILTERS)
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid')
  const deleteFile = useDeleteFile()

  const {
    data,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    isLoading,
    isError,
    error,
  } = useInfiniteFiles(filters)

  const files = data?.pages.flatMap((p) => p.files) ?? []

  // Infinite scroll: intersection observer on sentinel element
  const sentinelRef = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    const sentinel = sentinelRef.current
    if (!sentinel) return
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && hasNextPage && !isFetchingNextPage) {
          fetchNextPage()
        }
      },
      { threshold: 0.1 }
    )
    observer.observe(sentinel)
    return () => observer.disconnect()
  }, [hasNextPage, isFetchingNextPage, fetchNextPage])

  const handleDelete = useCallback(
    (fileId: string) => {
      if (!window.confirm('Delete this file?')) return
      deleteFile.mutate(fileId)
    },
    [deleteFile]
  )

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      <header className="border-b border-zinc-800 px-4 py-2.5">
        <button
          onClick={() => navigate('/channels')}
          className="flex items-center gap-2 text-sm text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to channels
        </button>
      </header>

      <FileFilterBar
        filters={filters}
        onFiltersChange={setFilters}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
      />

      <main className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="flex items-center justify-center py-16">
            <Loader2 className="h-6 w-6 animate-spin text-zinc-600" />
          </div>
        ) : isError ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <p className="text-sm text-red-400">
              {error instanceof Error ? error.message : 'Failed to load files'}
            </p>
          </div>
        ) : files.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
            <div className="rounded-full bg-zinc-800/50 p-3 text-zinc-500">
              <FolderOpen className="h-5 w-5" />
            </div>
            <div>
              <h3 className="text-sm font-medium text-zinc-300">No files found</h3>
              <p className="mt-1 max-w-xs text-xs text-zinc-500">
                Uploaded files from your channels and conversations will appear here.
              </p>
            </div>
          </div>
        ) : viewMode === 'grid' ? (
          <div className="grid grid-cols-1 gap-3 p-4 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
            {files.map((file) => (
              <FileCard
                key={file.id}
                file={file}
                viewMode="grid"
                currentUserId={currentUserId}
                onDelete={handleDelete}
              />
            ))}
          </div>
        ) : (
          <div className="flex flex-col divide-y divide-zinc-900 py-2">
            {files.map((file) => (
              <FileCard
                key={file.id}
                file={file}
                viewMode="list"
                currentUserId={currentUserId}
                onDelete={handleDelete}
              />
            ))}
          </div>
        )}

        {/* Infinite scroll sentinel */}
        <div ref={sentinelRef} className="flex items-center justify-center py-4">
          {isFetchingNextPage && (
            <Loader2 className="h-5 w-5 animate-spin text-zinc-600" />
          )}
        </div>
      </main>
    </div>
  )
}
