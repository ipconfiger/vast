import { test, expect, type Page } from '@playwright/test'
import { registerUser, createChannel, getChannelIdFromUrl } from './helpers'
import type { Message } from '../src/types'

// Sender id that is guaranteed to differ from the logged-in user's id,
// so the auto-scroll effect classifies the message as "incoming" (isOwn=false).
const INCOMING_SENDER_ID = 'e2e-incoming-sender-not-me'

interface ScrollMetrics {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
  distanceFromBottom: number
}

async function getScrollMetrics(page: Page): Promise<ScrollMetrics | null> {
  return page.evaluate(() => {
    const el = document.querySelector('[data-testid="message-list-scroll"]') as HTMLElement | null
    if (!el) return null
    return {
      scrollTop: el.scrollTop,
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
      distanceFromBottom: el.scrollHeight - el.scrollTop - el.clientHeight,
    }
  })
}

function makeSeedMessages(channelId: string, count: number): Message[] {
  const base = Date.now() - count * 1000
  return Array.from({ length: count }, (_, i) => ({
    id: `seed-${channelId}-${i}`,
    msg_id: `seed-${channelId}-${i}`,
    channel_id: channelId,
    sender_id: INCOMING_SENDER_ID,
    sender_name: `SeedUser${i}`,
    sender_display_name: `Seed User ${i}`,
    msg_type: 'text',
    payload: { text: `Seed message ${i} — ${'x'.repeat(60)}` },
    thread_parent_id: null,
    deleted_at: null,
    created_at: new Date(base + i * 1000).toISOString(),
  }))
}

function makeIncomingMessage(channelId: string, label: string): Message {
  const id = `inc-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  return {
    id,
    msg_id: id,
    channel_id: channelId,
    sender_id: INCOMING_SENDER_ID,
    sender_name: 'IncomingBot',
    sender_display_name: 'Incoming Bot',
    msg_type: 'text',
    payload: { text: `Incoming: ${label}` },
    thread_parent_id: null,
    deleted_at: null,
    created_at: new Date().toISOString(),
  }
}

/**
 * Drives the channel page into a known state: logged in, on a channel whose
 * MessageList has `seedCount` messages and is scrolled to the bottom.
 *
 * Strategy: page.route mocks GET /channels/:id/messages to return a shared
 * mutable array. Seeding mutates that array, then a reload triggers the real
 * useMessages → setMessages code path. Runtime "incoming" messages are added
 * BOTH to the route array (so any react-query refetch returns the same state)
 * AND via window.__messageStore.addMessage (which fires the auto-scroll effect
 * — exactly the real WS new_msg → store → effect path).
 */
async function setupChannelAtBottom(
  page: Page,
  regPrefix: string,
  seedCount: number,
): Promise<{ channelId: string; messages: Message[] }> {
  const messages: Message[] = []

  await page.route('**/channels/*/messages', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ messages, next_cursor: 0, has_more: false }),
    })
  })

  await registerUser(page, regPrefix)
  await createChannel(page, `Scroll-${regPrefix}`)
  await page.waitForSelector('.channel-item')
  await page.locator('.channel-item').first().click()
  await page.waitForURL(/\/channels\/[^/]+/, { timeout: 5000 })

  const channelId = getChannelIdFromUrl(page)
  expect(channelId, 'channel id must be parsed from url').toBeTruthy()

  messages.push(...makeSeedMessages(channelId, seedCount))
  await page.reload()
  await page.waitForSelector('[data-testid="message-list-scroll"]', { timeout: 5000 })
  await page.waitForSelector('.message-bubble', { timeout: 5000 })
  await page.waitForTimeout(600)

  return { channelId, messages }
}

async function injectIncoming(page: Page, messages: Message[], msg: Message): Promise<void> {
  messages.push(msg)
  await page.evaluate((m) => {
    const store = (window as unknown as { __messageStore?: { getState: () => { addMessage: (cid: string, mm: Message) => void } } }).__messageStore
    if (!store) throw new Error('__messageStore dev hook missing — not running against a DEV build')
    store.getState().addMessage(m.channel_id, m)
  }, msg)
}

test.describe('chat scroll behavior (regression for fix-chat-scroll-fight)', () => {
  test('stick-to-bottom preserved when at bottom and new message arrives', async ({ page }) => {
    const { channelId, messages } = await setupChannelAtBottom(page, 'scrlstick', 30)

    const atBottom = await getScrollMetrics(page)
    expect(atBottom, 'scroll container must render').not.toBeNull()
    // Sanity: list overflows the viewport (otherwise the test proves nothing).
    expect(atBottom!.scrollHeight, 'list must overflow viewport').toBeGreaterThan(
      atBottom!.clientHeight + 200,
    )
    expect(atBottom!.distanceFromBottom, 'must start at bottom').toBeLessThan(80)

    await injectIncoming(page, messages, makeIncomingMessage(channelId, 'mid-convo 1'))
    await page.waitForTimeout(600)

    const after = await getScrollMetrics(page)
    expect(after!.distanceFromBottom, 'view stays at bottom when user was at bottom').toBeLessThan(80)
  })

  test('scroll-up wins — user is not yanked back to bottom (core bug regression)', async ({ page }) => {
    const { channelId, messages } = await setupChannelAtBottom(page, 'scrlup', 30)

    const atBottom = await getScrollMetrics(page)
    expect(atBottom, 'scroll container must render').not.toBeNull()
    expect(atBottom!.scrollHeight, 'list must overflow viewport').toBeGreaterThan(
      atBottom!.clientHeight + 200,
    )
    expect(atBottom!.distanceFromBottom, 'must start at bottom').toBeLessThan(80)

    // First incoming message while user is at bottom — stick-to-bottom fires once.
    await injectIncoming(page, messages, makeIncomingMessage(channelId, 'trigger-stick'))
    await page.waitForTimeout(500)

    // User scrolls up by ~600px — the scroll listener must flip isAtBottomRef to false.
    const container = page.locator('[data-testid="message-list-scroll"]')
    await container.hover()
    await page.mouse.wheel(0, -600)
    await page.waitForTimeout(400)

    const afterScrollUp = await getScrollMetrics(page)
    expect(afterScrollUp!.distanceFromBottom, 'user scroll-up must move the view away from bottom').toBeGreaterThan(
      200,
    )

    // Second incoming message — the auto-scroll effect MUST NOT fire (isAtBottomRef false).
    await injectIncoming(page, messages, makeIncomingMessage(channelId, 'should-not-stick'))
    await page.waitForTimeout(600)

    const final = await getScrollMetrics(page)
    expect(
      final!.distanceFromBottom,
      'view must NOT be pushed back to bottom — user scroll wins',
    ).toBeGreaterThan(200)
  })
})
