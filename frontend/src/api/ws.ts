import { useEffect, useRef } from 'react'
import { useAuthStore } from '../stores/authStore'
import { usePresenceStore } from '../stores/presenceStore'
import { useReactionStore } from '../stores/reactionStore'
import type { Reaction } from '../types'

interface WsReactionEvent {
  type: 'reaction_added' | 'reaction_removed'
  message_id: string
  reaction: Reaction
}

interface WsTypingEvent {
  type: 'typing_start' | 'typing_stop'
  channel_id: string
  user_id: string
}

interface WsPresenceEvent {
  type: 'user_online' | 'user_offline'
  user_id: string
}

type WsMessage = WsReactionEvent | WsTypingEvent | WsPresenceEvent

function getWsUrl(): string {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/ws`
}

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const token = useAuthStore((s) => s.token)

  useEffect(() => {
    if (!token) return

    let destroyed = false

    function connect() {
      if (destroyed) return

      const ws = new WebSocket(getWsUrl())
      wsRef.current = ws

      ws.onopen = (_ev: Event) => {
        ws.send(JSON.stringify({ type: 'auth', token }))
      }

      ws.onmessage = (event) => {
        try {
          const data: WsMessage = JSON.parse(event.data as string)
          handleWsMessage(data)
        } catch {
          // ignore parse errors
        }
      }

      ws.onclose = () => {
        if (!destroyed) {
          reconnectTimeoutRef.current = setTimeout(connect, 3000)
        }
      }

      ws.onerror = () => {
        ws.close()
      }
    }

    connect()

    return () => {
      destroyed = true
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current)
      }
      wsRef.current?.close()
    }
  }, [token])
}

function handleWsMessage(data: WsMessage): void {
  switch (data.type) {
    case 'reaction_added': {
      const { addReaction } = useReactionStore.getState()
      addReaction(data.message_id, data.reaction)
      break
    }
    case 'reaction_removed': {
      const { removeReaction } = useReactionStore.getState()
      removeReaction(data.message_id, data.reaction.id)
      break
    }
    case 'typing_start': {
      const { addTyping } = usePresenceStore.getState()
      addTyping(data.channel_id, data.user_id)
      break
    }
    case 'typing_stop': {
      const { removeTyping } = usePresenceStore.getState()
      removeTyping(data.channel_id, data.user_id)
      break
    }
    case 'user_online': {
      const { addOnline } = usePresenceStore.getState()
      addOnline(data.user_id)
      break
    }
    case 'user_offline': {
      const { removeOnline } = usePresenceStore.getState()
      removeOnline(data.user_id)
      break
    }
  }
}
