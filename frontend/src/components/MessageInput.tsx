import { useState, useRef, useEffect, type KeyboardEvent } from 'react'
import { Send, Loader2, Code2, Paperclip } from 'lucide-react'
import { useSendMessage } from '../api/channels'
import { useUploadFile } from '../api/files'
import { CodeSnippetInput } from './CodeSnippetInput'
import { VoteBuilderModal } from './VoteBuilderModal'

type CmdRole = 'owner' | 'admin' | 'member'

const COMMANDS = [
  { cmd: 'quit', desc: 'Delete channel', args: false, requiredRole: 'owner' as CmdRole },
  { cmd: 'list', desc: 'List members', args: false, requiredRole: 'owner' as CmdRole },
  { cmd: 'kick', desc: 'Kick a member', args: true, argHint: '<username>', requiredRole: 'owner' as CmdRole },
  { cmd: 'train', desc: '发起接龙', args: true, argHint: '<标题>' },
  { cmd: 'vote', desc: '发起投票', args: true, argHint: '<标题>' },
]

const ROLE_HIERARCHY: Record<CmdRole, number> = { owner: 3, admin: 2, member: 1 }

function hasRole(userRole: string | undefined, required: CmdRole): boolean {
  if (!userRole) return false
  return (ROLE_HIERARCHY[userRole as CmdRole] ?? 0) >= ROLE_HIERARCHY[required]
}

interface MessageInputProps {
  channelId: string
  currentRole?: string
}

export function MessageInput({ channelId, currentRole }: MessageInputProps) {
  const [text, setText] = useState('')
  const [showCodeInput, setShowCodeInput] = useState(false)
  const [voteModalOpen, setVoteModalOpen] = useState(false)
  const [voteInitialTitle, setVoteInitialTitle] = useState('')
  const [commandIndex, setCommandIndex] = useState(-1)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const commandListRef = useRef<HTMLDivElement>(null)
  const sendMessage = useSendMessage(channelId)
  const uploadFile = useUploadFile(channelId)

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 160)}px`
    }
  }, [text])

  const handleSend = () => {
    const trimmed = text.trim()
    if (!trimmed || sendMessage.isPending) return

    if (trimmed.startsWith('/')) {
      const parts = trimmed.slice(1).split(/\s+/)
      const cmd = parts[0]
      const args = parts.slice(1).join(' ')
      const cmdDef = COMMANDS.find(c => c.cmd === cmd)
      if (cmdDef?.args && !args) return

      if (cmd === 'vote') {
        setVoteInitialTitle(args)
        setVoteModalOpen(true)
        setText('')
        if (textareaRef.current) {
          textareaRef.current.style.height = 'auto'
        }
        return
      }

      sendMessage.mutate({ msg_type: 'text', payload: { _command: true, command: cmd, args } })
    } else {
      sendMessage.mutate({ msg_type: 'text', payload: { text: trimmed } })
    }
    setText('')

    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (showCommands && filteredCommands.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setCommandIndex(i => i < filteredCommands.length - 1 ? i + 1 : 0)
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setCommandIndex(i => i > 0 ? i - 1 : filteredCommands.length - 1)
        return
      }
      if (e.key === 'Tab') {
        e.preventDefault()
        const idx = commandIndex >= 0 ? commandIndex : 0
        selectCommand(filteredCommands[idx])
        return
      }
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const handleCodeSend = (payload: { language: string; code: string; filename?: string }) => {
    sendMessage.mutate({ msg_type: 'code', payload })
  }

  const handleVoteConfirm = (title: string, options: string[]) => {
    sendMessage.mutate({
      msg_type: 'text',
      payload: { _vote_request: true, title, options },
    })
  }

  const showCommands = text.startsWith('/')
  const cmdFilter = text.startsWith('/') ? text.slice(1).split(/\s+/)[0].toLowerCase() : ''
  const filteredCommands = showCommands
    ? COMMANDS.filter(c => c.cmd.startsWith(cmdFilter) && (!c.requiredRole || hasRole(currentRole, c.requiredRole)))
    : []

  // Reset selection when filtered list changes (e.g. typing filters results)
  useEffect(() => {
    setCommandIndex(-1)
  }, [cmdFilter])

  useEffect(() => {
    // Scroll selected item into view
    if (commandIndex >= 0 && commandListRef.current) {
      const items = commandListRef.current.children
      if (items[commandIndex]) {
        items[commandIndex].scrollIntoView({ block: 'nearest' })
      }
    }
  }, [commandIndex])

  const selectCommand = (cmd: (typeof COMMANDS)[number]) => {
    setText(cmd.args ? `/${cmd.cmd} ` : `/${cmd.cmd}`)
    setCommandIndex(-1)
    textareaRef.current?.focus()
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
      <VoteBuilderModal
        isOpen={voteModalOpen}
        onClose={() => setVoteModalOpen(false)}
        onConfirm={handleVoteConfirm}
        initialTitle={voteInitialTitle}
      />
      <input
        type="file"
        ref={fileInputRef}
        onChange={handleFileChange}
        className="hidden"
        aria-label="Attach file"
      />
      <div className="message-input border-t border-zinc-800 bg-zinc-900/80 px-4 py-3">
        <div className="flex items-end gap-2 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 focus-within:border-zinc-500 transition-colors relative">
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
          {showCommands && filteredCommands.length > 0 && (
            <div ref={commandListRef} className="absolute left-0 bottom-full mb-1 z-50 rounded-lg border border-zinc-700 bg-zinc-800 shadow-xl py-1 min-w-[200px]">
              {filteredCommands.map((c, i) => (
                <button
                  key={c.cmd}
                  className={`flex items-center gap-2 w-full px-3 py-1.5 text-left text-sm text-zinc-300 ${i === commandIndex ? 'bg-zinc-700 text-zinc-100' : 'hover:bg-zinc-700'}`}
                  onMouseEnter={() => setCommandIndex(i)}
                  onClick={() => selectCommand(c)}
                >
                  <span className="text-indigo-400 font-mono text-xs">/{c.cmd}</span>
                  {c.args && <span className="text-zinc-500 text-xs">{c.argHint}</span>}
                  <span className="ml-auto text-xs text-zinc-500">{c.desc}</span>
                </button>
              ))}
            </div>
          )}
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
