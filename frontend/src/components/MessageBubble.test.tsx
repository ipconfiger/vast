import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, cleanup } from '@testing-library/react'
import { createElement } from 'react'
import type { Message } from '../types'

// Mock child components that pull in network/state so MessageBubble
// can be rendered in isolation. We only care about bot UI surfacing.
vi.mock('./ReactionPicker', () => ({
  ReactionPicker: () => createElement('div', { 'data-testid': 'reaction-picker' }),
}))
vi.mock('./ReactionBar', () => ({
  ReactionBar: () => createElement('div', { 'data-testid': 'reaction-bar' }),
}))
vi.mock('./UserAvatar', () => ({
  UserAvatar: () => createElement('div', { 'data-testid': 'user-avatar' }),
}))
// TextMessage is the actual renderer for case 'text'; keep the real one to
// verify whitespace-pre-wrap end-to-end.

import { MessageBubble } from './MessageBubble'

function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm-1',
    msg_id: 'm-1',
    channel_id: 'ch-1',
    sender_id: 'u-other',
    msg_type: 'text',
    payload: { text: 'hi' },
    created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

interface RenderOpts {
  isOwn?: boolean
  senderName?: string
}

function renderBubble(msg: Message, opts: RenderOpts = {}) {
  return render(
    createElement(MessageBubble, {
      message: msg,
      isOwn: opts.isOwn ?? false,
      senderName: opts.senderName ?? 'alice',
      senderAvatar: undefined,
      timestamp: '12:00 PM',
      channelId: 'ch-1',
    }),
  )
}

describe('MessageBubble bot rendering', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    cleanup()
  })

  it('renders the bot icon next to the sender name when message.is_bot is true', () => {
    const { container, getByText } = renderBubble(
      makeMessage({ is_bot: true, payload: { text: 'beep boop' } }),
      { senderName: 'hermes' },
    )
    // Bot icon = an SVG with aria-label "Bot"
    const botIcon = container.querySelector('svg[aria-label="Bot"]')
    expect(botIcon).toBeTruthy()
    // Sender name still renders
    expect(getByText('hermes')).toBeTruthy()
  })

  it('applies the indigo background tint to the bubble for bot messages', () => {
    const { container } = renderBubble(
      makeMessage({ is_bot: true, payload: { text: 'hi' } }),
    )
    const bubble = container.querySelector('.message-bubble')
    expect(bubble).toBeTruthy()
    expect(bubble?.className).toContain('bg-indigo-950/30')
    expect(bubble?.className).toContain('border-indigo-500/40')
  })

  it('colors the bot sender name with the indigo token', () => {
    const { getByText } = renderBubble(
      makeMessage({ is_bot: true, payload: { text: 'hi' } }),
      { senderName: 'hermes' },
    )
    const name = getByText('hermes')
    expect(name.className).toContain('text-indigo-400')
    expect(name.className).not.toContain('text-zinc-200')
  })

  it('renders NO bot icon, NO indigo background, and default name color for regular user messages', () => {
    const { container, getByText } = renderBubble(
      makeMessage({ is_bot: false, payload: { text: 'hi' } }),
      { senderName: 'alice' },
    )
    expect(container.querySelector('svg[aria-label="Bot"]')).toBeNull()
    const bubble = container.querySelector('.message-bubble')
    expect(bubble?.className).not.toContain('bg-indigo-950/30')
    const name = getByText('alice')
    expect(name.className).toContain('text-zinc-200')
    expect(name.className).not.toContain('text-indigo-400')
  })

  it('renders NO bot icon when is_bot is undefined (back-compat with pre-T1 messages)', () => {
    const { container } = renderBubble(makeMessage({ payload: { text: 'hi' } }))
    expect(container.querySelector('svg[aria-label="Bot"]')).toBeNull()
  })

  it('preserves line breaks in bot messages via whitespace-pre-wrap (renders TextMessage)', () => {
    const multiLine = 'line one\nline two\nline three'
    const { container } = renderBubble(
      makeMessage({ is_bot: true, payload: { text: multiLine } }),
    )
    const textEl = container.querySelector('.text-message')
    expect(textEl).toBeTruthy()
    expect(textEl?.className).toContain('whitespace-pre-wrap')
  })

  it('does not show bot UI when the message is the current user (isOwn=true)', () => {
    // Even if is_bot somehow lands on the current user's message, the bot
    // chrome must not appear next to "You".
    const { container, getByText } = renderBubble(
      makeMessage({ is_bot: true, sender_id: 'u-me', payload: { text: 'hi' } }),
      { isOwn: true, senderName: 'You' },
    )
    expect(container.querySelector('svg[aria-label="Bot"]')).toBeNull()
    const bubble = container.querySelector('.message-bubble')
    expect(bubble?.className).not.toContain('bg-indigo-950/30')
    const name = getByText('You')
    expect(name.className).toContain('text-zinc-200')
  })
})
