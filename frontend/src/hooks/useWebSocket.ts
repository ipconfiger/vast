import { useEffect, useState } from 'react'
import { useAuthStore } from '../stores/authStore'

const WS_URL = 'ws://localhost:3000/ws'
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
  private _status: WsStatus = 'disconnected'

  /** Connect (or reconnect) with the given token. */
  connect(token: string): void {
    if (this.ws && this.token === token && this._status === 'connected') return
    this.token = token
    this.retryCount = 0
    this.cancelRetry()
    this.doConnect()
  }

  /** Tear down the connection. */
  disconnect(): void {
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
    this.retryTimer = setTimeout(() => this.doConnect(), delay)
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
 * Use `getWsManager()` to subscribe to events outside of React components.
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

  return { status }
}
