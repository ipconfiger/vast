import { createElement, useEffect, useState } from 'react'
import { useAuthStore } from '../stores/authStore'
import { usePresenceStore } from '../stores/presenceStore'
import { useReactionStore } from '../stores/reactionStore'
import { useChannelStore } from '../stores/channelStore'
import { refreshAccessToken } from '../api/client'
import { queryClient } from '../queryClient'
import { toast, useToastStore } from '../stores/toastStore'
import type { Reaction } from '../types'

/** Lazy WS URL — not a module-level constant so tests importing the module don't crash in environments without `window`. */
function wsUrl(): string {
  return `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/ws`
}
const MAX_RETRIES = 5
const BASE_DELAY = 1000

export type WsStatus = 'disconnected' | 'connecting' | 'connected'
type Listener = (data: unknown) => void
type StatusListener = (status: WsStatus) => void

/** Manages a single global WebSocket connection with auto-reconnect. */
export class WebSocketManager {
  private ws: WebSocket | null = null
  private token: string | null = null
  private retryCount = 0
  private retryTimer: ReturnType<typeof setTimeout> | null = null
  private listeners = new Map<string, Set<Listener>>()
  private reconnectCallbacks = new Set<() => void>()
  private statusListeners = new Set<StatusListener>()
  private pendingSubscribes = new Set<string>()
  private _status: WsStatus = 'disconnected'

  /** Connect (or reconnect) with the given token. */
  connect(token: string): void {
    const alreadyConnected = this.ws && this.token === token && this._status === 'connected'
    const alreadyConnecting = this.token === token && this._status === 'connecting'
    if (alreadyConnected || alreadyConnecting) return
    console.log('[WS] connect called, closing prev:', !!this.ws, 'status:', this._status)
    this.token = token
    this.retryCount = 0
    this.cancelRetry()
    this.doConnect()
  }

  /** Subscribe the connection to a channel so WS events are delivered. */
  subscribeChannel(channelId: string): void {
    this.pendingSubscribes.add(channelId)
    if (!this.ws || this._status !== 'connected') return
    this.ws.send(JSON.stringify({ type: 'subscribe', channel_id: channelId }))
  }

  /** Unsubscribe from a channel. */
  unsubscribeChannel(channelId: string): void {
    this.pendingSubscribes.delete(channelId)
    if (!this.ws || this._status !== 'connected') return
    this.ws.send(JSON.stringify({ type: 'unsubscribe', channel_id: channelId }))
  }

  /** Tear down the connection. */
  disconnect(): void {
    console.log('[WS] disconnect called')
    this.cancelRetry()
    this.ws?.close()
    this.ws = null
    this.token = null
    this.retryCount = 0
    this.setStatus('disconnected')
  }

  /** Subscribe to a specific message type. Returns an unsubscribe function. */
  subscribe(type: string, listener: Listener): () => void {
    if (!this.listeners.has(type)) {
      this.listeners.set(type, new Set())
    }
    this.listeners.get(type)!.add(listener)
    return () => {
      this.listeners.get(type)?.delete(listener)
    }
  }

  /** Register a callback invoked after the connection re-establishes following a drop. */
  onReconnect(cb: () => void): () => void {
    this.reconnectCallbacks.add(cb)
    return () => {
      this.reconnectCallbacks.delete(cb)
    }
  }

  /** Listen for status changes (fires immediately with current status). */
  listenStatus(cb: StatusListener): () => void {
    this.statusListeners.add(cb)
    cb(this._status)
    return () => {
      this.statusListeners.delete(cb)
    }
  }

  get status(): WsStatus {
    return this._status
  }

  private doConnect(): void {
    if (!this.token) return
    this.setStatus('connecting')
    this.ws?.close()
    const ws = new WebSocket(`${wsUrl()}?token=${this.token}`)
    this.ws = ws

    ws.onopen = () => {
      if (this.ws !== ws) return
      const isReconnect = this.retryCount > 0
      this.retryCount = 0
      this.setStatus('connected')
      this.pendingSubscribes.forEach((ch) => {
        ws.send(JSON.stringify({ type: 'subscribe', channel_id: ch }))
      })
      if (isReconnect) {
        this.reconnectCallbacks.forEach((fn) => fn())
      }
    }

    ws.onclose = () => {
      if (this.ws !== ws) return
      this.setStatus('disconnected')
      this.scheduleReconnect()
    }

    ws.onerror = () => {
      // onclose always follows; no duplicate handling needed here
    }

    ws.onmessage = (event: MessageEvent) => {
      let parsed: { type: string; data?: unknown }
      try {
        parsed = JSON.parse(event.data as string)
      } catch {
        return
      }
      if (!parsed || typeof parsed.type !== 'string') return
      console.log('[WS] recv:', parsed.type, (parsed as any).channel_id?.slice(0, 8) || '')
      const typeListeners = this.listeners.get(parsed.type)
      if (typeListeners) {
        const payload = parsed.data ?? parsed
        typeListeners.forEach((fn) => fn(payload))
      }
    }
  }

  private scheduleReconnect(): void {
    if (this.retryCount >= MAX_RETRIES) return
    const delay = Math.min(BASE_DELAY * Math.pow(2, this.retryCount), 16000)
    this.retryCount++
    this.retryTimer = setTimeout(async () => {
      const prevToken = this.token
      await refreshAccessToken()
      if (this.token === prevToken && this.token) {
        this.doConnect()
      }
    }, delay)
  }

  private setStatus(s: WsStatus): void {
    this._status = s
    this.statusListeners.forEach((fn) => fn(s))
  }

  private cancelRetry(): void {
    if (this.retryTimer !== null) {
      clearTimeout(this.retryTimer)
      this.retryTimer = null
    }
  }
}

// ── Singleton ────────────────────────────────────────────────────────

let instance: WebSocketManager | null = null

export function getWsManager(): WebSocketManager {
  if (!instance) instance = new WebSocketManager()
  return instance
}

// ── React hook ───────────────────────────────────────────────────────

/**
 * React hook that drives the global WebSocket connection lifecycle based
 * on the auth token. Returns the current connection status.
 *
 * Also wires WS events into the presence/reaction stores. Use `getWsManager()`
 * to subscribe to additional events outside of React components.
 */
export function useWebSocket(): { status: WsStatus } {
  const token = useAuthStore((s) => s.token)
  const [status, setStatus] = useState<WsStatus>('disconnected')
  const manager = getWsManager()

  // Follow status changes
  useEffect(() => {
    const unsub = manager.listenStatus(setStatus)
    return unsub
  }, [manager])

  // Connect / disconnect based on auth state
  useEffect(() => {
    if (token) {
      manager.connect(token)
    } else {
      manager.disconnect()
    }
  }, [token, manager])

  useWsEventSync(manager)

  return { status }
}

/**
 * Subscribes the global presence/reaction stores to WS events emitted by
 * the singleton manager. Each mount registers its own subscriptions and
 * cleans up on unmount.
 */
function useWsEventSync(manager: WebSocketManager): void {
  useEffect(() => {
    const unsubs: Array<() => void> = [
      manager.subscribe('reaction_update', (data) => {
        const ev = data as { message_cursor?: number; reactions?: Array<{ emoji: string; count: number }> }
        if (!ev || ev.message_cursor == null || !ev.reactions) return
        console.log('[WS] reaction_update for msg', ev.message_cursor, ev.reactions)
        const msgId = String(ev.message_cursor)
        const reactions = ev.reactions.map((r: { emoji: string; count: number }) => ({
          id: `${msgId}_${r.emoji}`,
          message_id: msgId,
          user_id: '',
          emoji: r.emoji,
          created_at: '',
        }))
        useReactionStore.getState().setReactions(msgId, reactions)
      }),
      manager.subscribe('reaction_added', (data) => {
        const ev = data as { message_id: string; reaction: Reaction }
        if (!ev || typeof ev.message_id !== 'string' || !ev.reaction) return
        useReactionStore.getState().addReaction(ev.message_id, ev.reaction)
      }),
      manager.subscribe('reaction_removed', (data) => {
        const ev = data as { message_id: string; reaction: Reaction }
        if (!ev || typeof ev.message_id !== 'string' || !ev.reaction) return
        useReactionStore.getState().removeReaction(ev.message_id, ev.reaction.id)
      }),
      manager.subscribe('file_deleted', (data) => {
        const ev = data as { file_id: string; channel_id: string }
        if (!ev || typeof ev.file_id !== 'string') return
        window.dispatchEvent(new CustomEvent('file-deleted', { detail: ev }))
        queryClient.invalidateQueries({ queryKey: ['files'] })
        if (ev.channel_id) {
          queryClient.invalidateQueries({ queryKey: ['messages', ev.channel_id] })
        }
      }),
      manager.subscribe('typing_start', (data) => {
        const ev = data as { channel_id: string; user_id: string }
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.user_id !== 'string') return
        usePresenceStore.getState().addTyping(ev.channel_id, ev.user_id)
      }),
      manager.subscribe('typing_stop', (data) => {
        const ev = data as { channel_id: string; user_id: string }
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.user_id !== 'string') return
        usePresenceStore.getState().removeTyping(ev.channel_id, ev.user_id)
      }),
      manager.subscribe('user_online', (data) => {
        const ev = data as { user_id: string }
        if (!ev || typeof ev.user_id !== 'string') return
        usePresenceStore.getState().addOnline(ev.user_id)
      }),
      manager.subscribe('user_offline', (data) => {
        const ev = data as { user_id: string }
        if (!ev || typeof ev.user_id !== 'string') return
        usePresenceStore.getState().removeOnline(ev.user_id)
      }),
      manager.subscribe('join_request', (data) => {
        const ev = data as { channel_id: string; user_id: string; username?: string }
        if (!ev || typeof ev.channel_id !== 'string') return
        queryClient.invalidateQueries({ queryKey: ['messages', ev.channel_id] })
        useToastStore.getState().addToast({
          type: 'info',
          message: `@${ev.username || 'Someone'} requested to join a channel`,
          action: createElement(
            'button',
            {
              onClick: () => {
                window.history.pushState(null, '', '/channels/' + ev.channel_id)
                window.location.reload()
              },
            },
            'Open',
          ),
        })
      }),
      manager.subscribe('dm_created', () => {
        queryClient.invalidateQueries({ queryKey: ['dms'] })
      }),
      manager.subscribe('dm_closed', (data) => {
        const ev = data as { dm_channel_id: string }
        if (!ev || typeof ev.dm_channel_id !== 'string') return
        queryClient.invalidateQueries({ queryKey: ['dms'] })
        if (window.location.pathname.startsWith(`/channels/${ev.dm_channel_id}`)) {
          window.location.href = '/'
        }
      }),
      manager.subscribe('member_added', (data) => {
        const ev = data as { channel_id: string; user_id: string; username?: string }
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.user_id !== 'string') return
        const myId = useAuthStore.getState().user?.id
        if (ev.user_id === myId) {
          queryClient.refetchQueries({ queryKey: ['channels'] })
          queryClient.refetchQueries({ queryKey: ['discover-channels'] })
          toast.info('You have been added to a new channel!')
          setTimeout(() => {
            window.history.pushState(null, '', `/channels/${ev.channel_id}`)
            window.location.reload()
          }, 1500)
        }
      }),
      manager.subscribe('member_removed', (data) => {
        const ev = data as { channel_id: string; user_id: string }
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.user_id !== 'string') return
        const myId = useAuthStore.getState().user?.id
        if (ev.user_id === myId) {
          if (typeof window !== 'undefined') {
            window.history.replaceState(null, '', '/channels')
          }
          toast.error('You have been removed from the channel.')
          setTimeout(() => window.location.reload(), 1500)
        }
      }),
      manager.subscribe('channel_archived', (data) => {
        const ev = data as { channel_id: string }
        if (!ev || typeof ev.channel_id !== 'string') return
        useChannelStore.getState().updateChannel(ev.channel_id, { is_archived: true })
        // Invalidate React Query caches so ChannelListPage (which reads from useChannel)
        // re-fetches and shows the archived view, not a stale chat UI.
        queryClient.invalidateQueries({ queryKey: ['channel', ev.channel_id] })
        queryClient.invalidateQueries({ queryKey: ['channels'] })
      }),
      manager.subscribe('channel_unarchived', (data) => {
        const ev = data as { channel_id: string }
        if (!ev || typeof ev.channel_id !== 'string') return
        useChannelStore.getState().updateChannel(ev.channel_id, { is_archived: false })
      }),
      manager.subscribe('new_msg', (data) => {
        console.log('[Notif] new_msg received', data)
        const ev = data as { channel_id: string; sender_id: string; preview?: string } | null
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.sender_id !== 'string') return
        if (typeof document !== 'undefined' && document.visibilityState !== 'hidden') {
          console.log('[Notif] skipped - tab visible, visibilityState:', document.visibilityState)
          return
        }
        if (typeof Notification !== 'undefined' && Notification.permission !== 'granted') return
        const myId = useAuthStore.getState().user?.id
        if (ev.sender_id === myId) return
        try {
          console.log('[Notif] showing notification for', ev.channel_id)
          const n = new Notification('New message', {
            body: ev.preview || '',
            data: { url: `/channels/${ev.channel_id}` },
          })
          n.onclick = () => {
            window.focus()
            window.location.href = (n.data as { url?: string })?.url || `/channels/${ev.channel_id}`
            n.close()
          }
        } catch {
          // Notification constructor may throw if permission revoked mid-flight
        }
      }),
      manager.subscribe('thread_reply', (data) => {
        console.log('[Notif] thread_reply received', data)
        const ev = data as { channel_id: string; sender_id: string; preview?: string } | null
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.sender_id !== 'string') return
        if (typeof document !== 'undefined' && document.visibilityState !== 'hidden') {
          console.log('[Notif] skipped - tab visible, visibilityState:', document.visibilityState)
          return
        }
        if (typeof Notification !== 'undefined' && Notification.permission !== 'granted') return
        const myId = useAuthStore.getState().user?.id
        if (ev.sender_id === myId) return
        try {
          console.log('[Notif] showing notification for', ev.channel_id)
          const n = new Notification('New reply', {
            body: ev.preview || '',
            data: { url: `/channels/${ev.channel_id}` },
          })
          n.onclick = () => {
            window.focus()
            window.location.href = (n.data as { url?: string })?.url || `/channels/${ev.channel_id}`
            n.close()
          }
        } catch {
          // Notification constructor may throw if permission revoked mid-flight
        }
      }),
    ]
    return () => {
      unsubs.forEach((unsub) => unsub())
    }
  }, [manager])
}
