import { lazy, Suspense } from 'react'

const Editor = lazy(() =>
  import('@monaco-editor/react').then((mod) => ({ default: mod.Editor }))
)

interface CodeMessageProps {
  language: string
  code: string
  filename?: string
}

function EditorLoading() {
  return (
    <div className="flex h-32 items-center justify-center rounded-md border border-zinc-700 bg-zinc-900">
      <span className="text-xs text-zinc-500">Loading editor...</span>
    </div>
  )
}

const editorOptions = {
  readOnly: true,
  minimap: { enabled: false },
  lineNumbersMinChars: 3,
  scrollBeyondLastLine: false,
  fontSize: 13,
  fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  lineHeight: 1.6,
  padding: { top: 12, bottom: 12 },
  automaticLayout: true,
  wordWrap: 'on',
  renderLineHighlight: 'none',
  overviewRulerLanes: 0,
  hideCursorInOverviewRuler: true,
  scrollbar: {
    vertical: 'hidden',
    horizontal: 'hidden',
    alwaysConsumeMouseWheel: false,
  },
} as const

export function CodeMessage({ language, code, filename }: CodeMessageProps) {
  const lineCount = code.split('\n').length
  const height = Math.min(Math.max(lineCount * 22 + 24, 60), 480)

  return (
    <div className="code-message overflow-hidden rounded-lg border border-zinc-700/60 bg-zinc-900">
      <div className="flex items-center gap-2 border-b border-zinc-700/40 px-3 py-1.5">
        <span className="rounded bg-zinc-700/50 px-1.5 py-0.5 font-mono text-[11px] text-zinc-400">
          {language}
        </span>
        {filename && (
          <span className="truncate text-xs text-zinc-500">{filename}</span>
        )}
      </div>
      <Suspense fallback={<EditorLoading />}>
        <Editor
          height={height}
          language={language}
          value={code}
          theme="vs-dark"
          options={editorOptions}
          loading={<EditorLoading />}
        />
      </Suspense>
    </div>
  )
}
