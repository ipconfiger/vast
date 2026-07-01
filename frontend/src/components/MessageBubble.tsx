import type { Message } from '../types'
import { TextMessage } from './TextMessage'
import { CodeMessage } from './CodeMessage'
import { ReactionPicker } from './ReactionPicker'
import { ReactionBar } from './ReactionBar'

function FileMessage() {
  return (
    <div className="file-message rounded-lg border border-zinc-700 bg-zinc-800/50 p-3">
      <span className="text-sm text-zinc-400">File attachment (preview coming soon)</span>
    </div>
  )
}

interface MessageBubbleProps {
  message: Message
  isOwn: boolean
  senderName: string
  senderAvatar?: string
  timestamp: string
}

export function MessageBubble({
  message,
  isOwn,
  senderName,
  senderAvatar,
  timestamp,
}: MessageBubbleProps) {
  const renderContent = () => {
    switch (message.msg_type) {
      case 'text':
        return <TextMessage text={typeof message.payload === 'string' ? message.payload : message.payload?.text ?? ''} />
      case 'file':
        return <FileMessage />
      case 'code':
        return <CodeMessage language={message.payload?.language ?? 'plaintext'} code={message.payload?.code ?? ''} filename={message.payload?.filename} />
      default:
        return <TextMessage text={typeof message.payload === 'string' ? message.payload : JSON.stringify(message.payload)} />
    }
  }

  return (
    <div className="message-bubble group flex gap-3 px-4 py-2 hover:bg-zinc-800/30">
      <div className="flex-shrink-0 pt-0.5">
        {senderAvatar ? (
          <img
            src={senderAvatar}
            alt={senderName}
            className="h-9 w-9 rounded-md object-cover"
          />
        ) : (
          <div className="flex h-9 w-9 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold text-zinc-300">
            {senderName.charAt(0).toUpperCase()}
          </div>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <div className="flex items-baseline gap-2 min-w-0">
            <span className="font-semibold text-sm text-zinc-200">
              {senderName}
            </span>
            <span className="text-xs text-zinc-500 opacity-0 group-hover:opacity-100 transition-opacity">
              {timestamp}
            </span>
          </div>
          <div className="ml-auto flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
            <ReactionPicker messageId={message.id || message.msg_id} isOwn={isOwn} />
          </div>
        </div>
        <div className="mt-0.5 text-sm text-zinc-100 leading-relaxed">
          {renderContent()}
        </div>
        <ReactionBar messageId={message.id || message.msg_id} />
      </div>
    </div>
  )
}
