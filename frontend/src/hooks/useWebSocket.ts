import { useEffect, useState } from 'react'
import { useAuthStore } from '../stores/authStore'
import { usePresenceStore } from '../stores/presenceStore'
import { useReactionStore } from '../stores/reactionStore'
import { refreshAccessToken } from '../api/client'
import type { Reaction } from '../types'

// Build WebSocket URL from the current page origin so it works in any
// environment — dev (Vite proxy), production (same origin), or custom deployments.
const WS_URL = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/ws`
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
    const ws = new WebSocket(`${WS_URL}?token=${this.token}`)
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
      manager.subscribe('member_removed', (data) => {
        const ev = data as { channel_id: string; user_id: string }
        if (!ev || typeof ev.channel_id !== 'string' || typeof ev.user_id !== 'string') return
        const myId = useAuthStore.getState().user?.id
        if (ev.user_id === myId) {
          if (typeof window !== 'undefined') {
            window.history.replaceState(null, '', '/channels')
          }
          const container = document.querySelector('.toast-container')
          if (container) {
            const toast = document.createElement('div')
            toast.className = 'bg-red-600 text-white px-4 py-2 rounded-lg shadow-lg text-sm'
            toast.textContent = 'You have been removed from the channel.'
            container.appendChild(toast)
            setTimeout(() => toast.remove(), 5000)
            window.location.reload()
          }
        }
      }),
      manager.subscribe('channel_archived', async (data) => {
        const ev = data as { channel_id: string }
        if (!ev || typeof ev.channel_id !== 'string') return
        const store = (await import('../stores/channelStore')).useChannelStore.getState()
        if (store.currentChannelId === ev.channel_id) {
          store.setCurrentChannel(null)
          window.history.replaceState(null, '', '/channels')
          window.location.reload()
        }
      }),
    ]
    return () => {
      unsubs.forEach((unsub) => unsub())
    }
  }, [manager])
}
