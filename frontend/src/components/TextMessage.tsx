import { useState, useEffect } from 'react'
import { Loader2, ExternalLink } from 'lucide-react'

interface TextMessageProps {
  text: string
}

function toRawUrl(url: string): string {
  const gh = url.match(/^https?:\/\/github\.com\/([^\/]+\/[^\/]+)\/blob\/(.+)$/)
  if (gh) return `https://raw.githubusercontent.com/${gh[1]}/${gh[2]}`
  const gl = url.match(/^https?:\/\/([^\/]+)\/([^\/]+\/[^\/]+)\/-\/blob\/(.+)$/)
  if (gl) return `https://${gl[1]}/${gl[2]}/-/raw/${gl[3]}`
  return url
}

function isRawUrl(text: string): boolean {
  try {
    const url = new URL(text)
    const path = url.pathname.toLowerCase()
    const ext = path.split('.').pop()?.split('?')[0] || ''
    const codeExts = ['yaml', 'yml', 'json', 'toml', 'rs', 'ts', 'tsx', 'js', 'jsx', 'py', 'go', 'java', 'c', 'cpp', 'h', 'rb', 'php', 'sh', 'bash', 'md', 'txt', 'env', 'lock', 'ini', 'cfg', 'conf', 'css', 'html', 'xml', 'sql', 'graphql', 'proto', 'toml']
    return codeExts.includes(ext) || url.hostname.includes('raw') || url.pathname.includes('/raw/')
  } catch { return false }
}

function RawContentPreview({ url }: { url: string }) {
  const [content, setContent] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(false)

  useEffect(() => {
    setLoading(true)
    setError(false)
    fetch(toRawUrl(url))
      .then(r => { if (!r.ok) throw new Error(); return r.text() })
      .then(t => { setContent(t.slice(0, 50000)); setLoading(false) })
      .catch(() => { setError(true); setLoading(false) })
  }, [url])

  if (loading) return (
    <div className="mt-2 flex items-center gap-2 text-xs text-zinc-500">
      <Loader2 className="h-3 w-3 animate-spin" />
      Loading raw content...
    </div>
  )
  if (error || !content) return null

  const ext = url.split('.').pop()?.split('?')[0] || ''

  return (
    <div className="mt-2 rounded-lg border border-zinc-700 overflow-hidden max-w-full">
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-800 border-b border-zinc-700">
        <span className="text-xs text-zinc-400 font-mono">{ext}</span>
        <a href={url} target="_blank" rel="noopener noreferrer" className="flex items-center gap-1 text-xs text-zinc-400 hover:text-zinc-200 transition-colors">
          <ExternalLink className="h-3 w-3" />
          <span>open in new tab</span>
        </a>
      </div>
      <pre className="p-3 text-xs font-mono text-zinc-300 bg-zinc-900/50 overflow-x-auto overflow-y-auto max-h-96 whitespace-pre">
        <code>{content}</code>
      </pre>
    </div>
  )
}

export function TextMessage({ text }: TextMessageProps) {
  const trimmed = text.trim()
  const isUrl = isRawUrl(trimmed)

  const parts = text.split(/(@\w+)/g)

  return (
    <div>
      <span className="text-message whitespace-pre-wrap break-words">
        {parts.map((part, i) => {
          if (part.startsWith('@')) {
            return <span key={i} className="mention font-semibold">{part}</span>
          }
          return <span key={i}>{part}</span>
        })}
      </span>
      {isUrl && <RawContentPreview url={trimmed} />}
    </div>
  )
}
