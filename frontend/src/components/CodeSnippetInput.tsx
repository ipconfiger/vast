import { useState, lazy, Suspense } from 'react'
import { X, Send, ChevronDown } from 'lucide-react'

const Editor = lazy(() =>
  import('@monaco-editor/react').then((mod) => ({ default: mod.Editor }))
)

interface CodeSnippetInputProps {
  onSend: (payload: { language: string; code: string; filename?: string }) => void
  onClose: () => void
}

const LANGUAGES = [
  { value: 'typescript', label: 'TypeScript' },
  { value: 'javascript', label: 'JavaScript' },
  { value: 'python', label: 'Python' },
  { value: 'rust', label: 'Rust' },
  { value: 'go', label: 'Go' },
  { value: 'java', label: 'Java' },
  { value: 'cpp', label: 'C++' },
  { value: 'c', label: 'C' },
  { value: 'csharp', label: 'C#' },
  { value: 'html', label: 'HTML' },
  { value: 'css', label: 'CSS' },
  { value: 'json', label: 'JSON' },
  { value: 'yaml', label: 'YAML' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'sql', label: 'SQL' },
  { value: 'shell', label: 'Shell' },
  { value: 'ruby', label: 'Ruby' },
  { value: 'swift', label: 'Swift' },
  { value: 'kotlin', label: 'Kotlin' },
  { value: 'dart', label: 'Dart' },
]

function EditorLoading() {
  return (
    <div className="flex flex-1 items-center justify-center rounded-md border border-zinc-700 bg-zinc-900">
      <div className="flex flex-col items-center gap-2">
        <div className="h-5 w-5 animate-spin rounded-full border-2 border-zinc-600 border-t-zinc-300" />
        <span className="text-xs text-zinc-500">Loading editor...</span>
      </div>
    </div>
  )
}

export function CodeSnippetInput({ onSend, onClose }: CodeSnippetInputProps) {
  const [language, setLanguage] = useState('typescript')
  const [code, setCode] = useState('')
  const [filename, setFilename] = useState('')
  const [showLangDropdown, setShowLangDropdown] = useState(false)

  const selectedLabel = LANGUAGES.find((l) => l.value === language)?.label ?? language

  const handleSend = () => {
    const trimmed = code.trim()
    if (!trimmed) return
    onSend({
      language,
      code: trimmed,
      filename: filename.trim() || undefined,
    })
    onClose()
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (showLangDropdown) {
        setShowLangDropdown(false)
      } else {
        onClose()
      }
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onKeyDown={handleKeyDown}
    >
      <div className="mx-4 flex w-full max-w-3xl flex-col rounded-xl border border-zinc-700 bg-zinc-900 shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-700/50 px-4 py-3">
          <h2 className="text-sm font-medium text-zinc-200">Share Code Snippet</h2>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-300 transition-colors"
            aria-label="Close code editor"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Options bar */}
        <div className="flex items-center gap-3 border-b border-zinc-700/40 px-4 py-2.5">
          {/* Language selector */}
          <div className="relative">
            <button
              onClick={() => setShowLangDropdown(!showLangDropdown)}
              className="flex items-center gap-1.5 rounded-md border border-zinc-600 bg-zinc-800 px-2.5 py-1 text-xs text-zinc-300 hover:border-zinc-500 transition-colors"
            >
              <span>{selectedLabel}</span>
              <ChevronDown className="h-3 w-3 text-zinc-500" />
            </button>
            {showLangDropdown && (
              <div className="absolute left-0 top-full z-10 mt-1 max-h-52 w-44 overflow-y-auto rounded-md border border-zinc-600 bg-zinc-850 bg-zinc-800 py-1 shadow-xl">
                {LANGUAGES.map((lang) => (
                  <button
                    key={lang.value}
                    onClick={() => {
                      setLanguage(lang.value)
                      setShowLangDropdown(false)
                    }}
                    className={`w-full px-3 py-1.5 text-left text-xs transition-colors hover:bg-zinc-700 ${
                      lang.value === language
                        ? 'text-zinc-100 bg-zinc-700/50'
                        : 'text-zinc-400'
                    }`}
                  >
                    {lang.label}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Filename input */}
          <input
            type="text"
            value={filename}
            onChange={(e) => setFilename(e.target.value)}
            placeholder="filename.ext (optional)"
            className="flex-1 rounded-md border border-zinc-600 bg-zinc-800 px-2.5 py-1 text-xs text-zinc-300 placeholder-zinc-500 outline-none focus:border-zinc-500 transition-colors"
          />
        </div>

        {/* Editor */}
        <div className="flex h-80 flex-col">
          <Suspense fallback={<EditorLoading />}>
            <Editor
              height="100%"
              language={language}
              value={code}
              onChange={(value) => setCode(value ?? '')}
              theme="vs-dark"
              options={{
                minimap: { enabled: false },
                lineNumbersMinChars: 3,
                scrollBeyondLastLine: false,
                fontSize: 13,
                fontFamily:
                  "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
                lineHeight: 1.6,
                padding: { top: 12, bottom: 12 },
                automaticLayout: true,
                tabSize: 2,
              }}
              loading={<EditorLoading />}
            />
          </Suspense>
        </div>

        {/* Footer buttons */}
        <div className="flex items-center justify-end gap-2 border-t border-zinc-700/50 px-4 py-3">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-300 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSend}
            disabled={!code.trim()}
            className="flex items-center gap-1.5 rounded-md bg-zinc-100 px-3 py-1.5 text-xs font-medium text-zinc-900 hover:bg-white disabled:opacity-30 disabled:hover:bg-zinc-100 transition-colors"
          >
            <Send className="h-3 w-3" />
            Send Code
          </button>
        </div>
      </div>
    </div>
  )
}
