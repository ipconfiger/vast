import { FileText, Download, Trash2, Loader2 } from 'lucide-react'
import dayjs from 'dayjs'
import { useAuthImage } from '../hooks/useAuthImage'
import type { FileRecord } from '../types'

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}

interface FileCardProps {
  file: FileRecord
  viewMode: 'grid' | 'list'
  currentUserId: string
  onDelete?: (fileId: string) => void
}

export function FileCard({ file, viewMode, currentUserId, onDelete }: FileCardProps) {
  const isImage = file.mime_type.startsWith('image/')
  const imgSrc = useAuthImage(
    isImage && !file.is_deleted ? `/api/files/${file.id}` : null
  )
  const isOwner = file.uploader_id === currentUserId && !file.is_deleted

  if (viewMode === 'list') {
    return (
      <div
        className={`flex items-center gap-3 rounded-lg px-4 py-2.5 transition-colors hover:bg-zinc-800/40 ${
          file.is_deleted ? 'opacity-50' : ''
        }`}
      >
        <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md bg-zinc-800 text-zinc-400">
          {isImage && imgSrc ? (
            <img
              src={imgSrc}
              alt={file.original_name}
              className="h-8 w-8 rounded object-cover"
            />
          ) : (
            <FileText className="h-4 w-4" />
          )}
        </div>
        <span className="min-w-0 flex-1 truncate text-sm text-zinc-200">
          {file.original_name}
          {file.is_deleted && (
            <span className="ml-2 text-xs text-zinc-500">已被发布者删除</span>
          )}
        </span>
        <span className="flex-shrink-0 w-16 text-right text-xs text-zinc-500 tabular-nums">
          {formatSize(file.size)}
        </span>
        <span className="flex-shrink-0 w-12 text-center text-xs text-zinc-600 uppercase">
          {file.extension}
        </span>
        <span className="flex-shrink-0 w-24 truncate text-xs text-zinc-500">
          {file.channel_name || '-'}
        </span>
        <span className="flex-shrink-0 w-20 truncate text-xs text-zinc-500">
          {file.uploader_display_name || file.uploader_name}
        </span>
        <span className="flex-shrink-0 w-28 text-right text-xs text-zinc-500">
          {dayjs.unix(file.created_at).format('MMM D, h:mm A')}
        </span>
        <div className="flex flex-shrink-0 items-center gap-1">
          {!file.is_deleted && (
            <a
              href={`/api/files/${file.id}`}
              target="_blank"
              rel="noopener noreferrer"
              className="rounded p-1 text-zinc-500 hover:bg-zinc-700 hover:text-zinc-200"
              aria-label="Download"
            >
              <Download className="h-4 w-4" />
            </a>
          )}
          {isOwner && (
            <button
              onClick={(e) => {
                e.stopPropagation()
                onDelete?.(file.id)
              }}
              className="rounded p-1 text-zinc-500 hover:bg-red-900/50 hover:text-red-400"
              aria-label="Delete file"
            >
              <Trash2 className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>
    )
  }

  return (
    <div
      className={`relative flex flex-col rounded-lg border border-zinc-800 bg-zinc-900/60 p-3 transition-colors hover:border-zinc-700 ${
        file.is_deleted ? 'opacity-50' : ''
      }`}
    >
      {file.is_deleted && (
        <div className="absolute inset-0 z-10 flex items-center justify-center rounded-lg bg-zinc-800/50">
          <span className="text-xs text-zinc-400">已被发布者删除</span>
        </div>
      )}
      <div className="mb-3 flex items-start justify-between">
        <div className="flex h-12 w-12 items-center justify-center overflow-hidden rounded-lg bg-zinc-800 text-zinc-400">
          {isImage && imgSrc ? (
            <img
              src={imgSrc}
              alt={file.original_name}
              className="h-full w-full object-cover"
            />
          ) : isImage ? (
            <Loader2 className="h-5 w-5 animate-spin text-zinc-500" />
          ) : (
            <FileText className="h-6 w-6" />
          )}
        </div>
        <div className="flex gap-0.5">
          {!file.is_deleted && (
            <a
              href={`/api/files/${file.id}`}
              target="_blank"
              rel="noopener noreferrer"
              className="rounded p-1 text-zinc-500 hover:bg-zinc-700 hover:text-zinc-200"
              aria-label="Download"
            >
              <Download className="h-4 w-4" />
            </a>
          )}
          {isOwner && (
            <button
              onClick={(e) => {
                e.stopPropagation()
                onDelete?.(file.id)
              }}
              className="rounded p-1 text-zinc-500 hover:bg-red-900/50 hover:text-red-400"
              aria-label="Delete file"
            >
              <Trash2 className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>
      <p className="truncate text-sm font-medium text-zinc-200">
        {file.original_name}
      </p>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-zinc-500">
        <span className="tabular-nums">{formatSize(file.size)}</span>
        <span>·</span>
        <span className="truncate">{file.channel_name || '-'}</span>
        <span>·</span>
        <span className="truncate">{file.uploader_display_name || file.uploader_name}</span>
      </div>
      <p className="mt-1 text-xs text-zinc-600">
        {dayjs.unix(file.created_at).format('MMM D, YYYY h:mm A')}
      </p>
    </div>
  )
}
