import { useState, useRef, useEffect, type KeyboardEvent } from 'react'
import { Send, Loader2, Code2, Paperclip } from 'lucide-react'
import { useSendMessage } from '../api/channels'
import { useUploadFile } from '../api/files'
import { CodeSnippetInput } from './CodeSnippetInput'

interface MessageInputProps {
  channelId: string
}

export function MessageInput({ channelId }: MessageInputProps) {
  const [text, setText] = useState('')
  const [showCodeInput, setShowCodeInput] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const sendMessage = useSendMessage(channelId)
  const uploadFile = useUploadFile()

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 160)}px`
    }
  }, [text])

  const handleSend = () => {
    const trimmed = text.trim()
    if (!trimmed || sendMessage.isPending) return

    sendMessage.mutate({ msg_type: 'text', payload: { text: trimmed } })
    setText('')

    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const handleCodeSend = (payload: { language: string; code: string; filename?: string }) => {
    sendMessage.mutate({ msg_type: 'code', payload })
  }

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return

    uploadFile.mutate(file, {
      onSuccess: (data) => {
        sendMessage.mutate({
          msg_type: 'file',
          payload: {
            file_id: data.file_id,
            url: data.url,
            original_name: data.original_name,
            size: data.size,
            mime_type: data.mime_type,
          },
        })
      },
    })

    // Reset the input so the same file can be re-selected
    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }

  return (
    <>
      {showCodeInput && (
        <CodeSnippetInput
          onSend={handleCodeSend}
          onClose={() => setShowCodeInput(false)}
        />
      )}
      <input
        type="file"
        ref={fileInputRef}
        onChange={handleFileChange}
        className="hidden"
        aria-label="Attach file"
      />
      <div className="message-input border-t border-zinc-800 bg-zinc-900/80 px-4 py-3">
        <div className="flex items-end gap-2 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 focus-within:border-zinc-500 transition-colors">
          <button
            onClick={() => setShowCodeInput(true)}
            className="flex-shrink-0 rounded-md p-1.5 text-zinc-400 hover:text-zinc-100 hover:bg-zinc-700 transition-colors"
            aria-label="Share code snippet"
          >
            <Code2 className="h-4 w-4" />
          </button>
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={uploadFile.isPending}
            className="flex-shrink-0 rounded-md p-1.5 text-zinc-400 hover:text-zinc-100 hover:bg-zinc-700 disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-zinc-400 transition-colors"
            aria-label="Attach file"
          >
            {uploadFile.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Paperclip className="h-4 w-4" />
            )}
          </button>
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={`Message #${channelId.slice(0, 8)}...`}
            rows={1}
            className="flex-1 resize-none bg-transparent text-sm text-zinc-100 placeholder-zinc-500 outline-none"
          />
          <button
            onClick={handleSend}
            disabled={!text.trim() || sendMessage.isPending}
            className="flex-shrink-0 rounded-md p-1.5 text-zinc-400 hover:text-zinc-100 hover:bg-zinc-700 disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-zinc-400 transition-colors"
            aria-label="Send message"
          >
            {sendMessage.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Send className="h-4 w-4" />
            )}
          </button>
        </div>
      </div>
    </>
  )
}
