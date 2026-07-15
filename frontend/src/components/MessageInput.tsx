import { useState, useRef, useEffect, useMemo, type KeyboardEvent } from 'react'
import { Send, Loader2, Code2, Paperclip, Bot } from 'lucide-react'
import { useSendMessage, usePublicBots } from '../api/channels'
import { useUploadFile } from '../api/files'
import { useChannelMembers, type MemberResponse } from '../api/channels'
import { getUserDisplayName } from '../utils/user'
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
  const [showMentions, setShowMentions] = useState(false)
  const [mentionFilter, setMentionFilter] = useState('')
  const [mentionIndex, setMentionIndex] = useState(-1)
  const [mentionStartPos, setMentionStartPos] = useState(-1)
  const mentionListRef = useRef<HTMLDivElement>(null)
  const cursorPosRef = useRef(0)
  const skipNextKeyUpRef = useRef(false)

  const { data: members } = useChannelMembers(channelId)
  const { data: publicBots } = usePublicBots()
  const botIds = useMemo(() => new Set(publicBots?.map(b => b.id) ?? []), [publicBots])

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

  const computeMentionState = (text: string, cursorPos: number) => {
    const textBeforeCursor = text.slice(0, cursorPos)
    const atIndex = textBeforeCursor.lastIndexOf('@')

    if (atIndex === -1 || (atIndex > 0 && textBeforeCursor[atIndex - 1] !== ' ')) {
      setShowMentions(false)
      return
    }

    const filter = textBeforeCursor.slice(atIndex + 1)

    if (filter.includes(' ')) {
      setShowMentions(false)
      return
    }

    setMentionFilter(filter)
    setMentionStartPos(atIndex)
    setShowMentions(true)
  }

  const selectMention = (member: MemberResponse) => {
    if (mentionStartPos < 0) return

    const username = member.user?.username ?? 'unknown'
    const before = text.slice(0, mentionStartPos)
    const after = text.slice(cursorPosRef.current)
    const newText = `${before}@${username} ${after}`

    setText(newText)
    setShowMentions(false)
    setMentionIndex(-1)
    setMentionStartPos(-1)

    requestAnimationFrame(() => {
      if (textareaRef.current) {
        const newCursorPos = mentionStartPos + username.length + 2
        textareaRef.current.focus()
        textareaRef.current.setSelectionRange(newCursorPos, newCursorPos)
      }
    })
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

    if (showMentions && mentionFiltered.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        skipNextKeyUpRef.current = true
        setMentionIndex(i => (i < mentionFiltered.length - 1 ? i + 1 : 0))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        skipNextKeyUpRef.current = true
        setMentionIndex(i => (i > 0 ? i - 1 : mentionFiltered.length - 1))
        return
      }
      if (e.key === 'Tab') {
        e.preventDefault()
        const idx = mentionIndex >= 0 ? mentionIndex : 0
        if (mentionFiltered[idx]) {
          selectMention(mentionFiltered[idx])
        }
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        setShowMentions(false)
        return
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newText = e.target.value
    setText(newText)
    const cursorPos = e.target.selectionStart
    cursorPosRef.current = cursorPos
    computeMentionState(newText, cursorPos)
  }

  const handleTextareaClick = () => {
    const cursorPos = textareaRef.current?.selectionStart ?? 0
    cursorPosRef.current = cursorPos
    computeMentionState(text, cursorPos)
  }

  const handleKeyUp = (_e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (skipNextKeyUpRef.current) {
      skipNextKeyUpRef.current = false
      return
    }
    const cursorPos = textareaRef.current?.selectionStart ?? 0
    cursorPosRef.current = cursorPos
    computeMentionState(text, cursorPos)
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

  const mentionFiltered = useMemo(() => {
    if (!showMentions || mentionStartPos < 0) return []
    const query = mentionFilter.toLowerCase()
    return (members ?? []).filter(m => {
      const displayName = m.user?.display_name?.toLowerCase() ?? ''
      const username = m.user?.username?.toLowerCase() ?? ''
      return !query || displayName.includes(query) || username.includes(query)
    })
  }, [showMentions, mentionStartPos, mentionFilter, members])

  // Reset selection when filtered list changes (e.g. typing filters results)
  useEffect(() => {
    setCommandIndex(-1)
  }, [cmdFilter])

  useEffect(() => {
    setMentionIndex(-1)
  }, [mentionFilter])

  useEffect(() => {
    // Scroll selected item into view
    if (commandIndex >= 0 && commandListRef.current) {
      const items = commandListRef.current.children
      if (items[commandIndex]) {
        items[commandIndex].scrollIntoView({ block: 'nearest' })
      }
    }
  }, [commandIndex])

  useEffect(() => {
    if (mentionIndex >= 0 && mentionListRef.current) {
      const items = mentionListRef.current.children
      if (items[mentionIndex]) {
        items[mentionIndex].scrollIntoView({ block: 'nearest' })
      }
    }
  }, [mentionIndex])

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
          {(showCommands && filteredCommands.length > 0) || (showMentions && mentionFiltered.length > 0) ? (
            <div className="absolute left-0 bottom-full mb-1 z-50 flex flex-col-reverse gap-1">
              {showMentions && mentionFiltered.length > 0 && (
                <div ref={mentionListRef} className="rounded-lg border border-zinc-700 bg-zinc-800 shadow-xl py-1 min-w-[240px] max-h-[240px] overflow-y-auto">
                  {mentionFiltered.map((member, i) => {
                    const displayName = getUserDisplayName(member.user?.display_name, member.user?.username, member.user_id)
                    const username = member.user?.username ?? 'unknown'
                    const isBot = botIds.has(member.user_id)

                    const roleBadgeClass = member.role === 'owner'
                      ? 'text-amber-400 bg-amber-500/10 border-amber-500/20'
                      : member.role === 'admin'
                        ? 'text-indigo-400 bg-indigo-500/10 border-indigo-500/20'
                        : 'text-zinc-500 bg-zinc-500/10 border-zinc-500/20'

                    return (
                      <button
                        key={member.user_id}
                        className={`flex items-center gap-2 w-full px-3 py-2 text-left text-sm transition-colors ${
                          i === mentionIndex
                            ? 'bg-zinc-700 text-zinc-100'
                            : 'text-zinc-300 hover:bg-zinc-700'
                        }`}
                        onMouseEnter={() => setMentionIndex(i)}
                        onClick={() => selectMention(member)}
                      >
                        <div className="flex items-center justify-center h-6 w-6 rounded bg-zinc-700 font-semibold text-[10px] text-zinc-300 flex-shrink-0">
                          {isBot ? (
                            <Bot className="h-3.5 w-3.5 text-emerald-400" />
                          ) : (
                            displayName.charAt(0).toUpperCase()
                          )}
                        </div>
                        <div className="flex items-center gap-1.5 min-w-0 flex-1">
                          <span className="truncate">{displayName}</span>
                          <span className="text-zinc-500 text-xs flex-shrink-0">@{username}</span>
                        </div>
                        <span className={`inline-flex items-center rounded-md border px-1.5 py-0.5 text-[10px] font-medium flex-shrink-0 ${roleBadgeClass}`}>
                          {member.role}
                        </span>
                      </button>
                    )
                  })}
                </div>
              )}
              {showCommands && filteredCommands.length > 0 && (
                <div ref={commandListRef} className="rounded-lg border border-zinc-700 bg-zinc-800 shadow-xl py-1 min-w-[200px]">
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
            </div>
          ) : null}
          <textarea
            ref={textareaRef}
            value={text}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            onKeyUp={handleKeyUp}
            onClick={handleTextareaClick}
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
