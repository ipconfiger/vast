import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement } from 'react'
import AdminAuditLogsPage from './AdminAuditLogsPage'
import type { AuditLog } from '../../api/admin'

// --- Mocks -----------------------------------------------------------------

const listAuditLogsMock = vi.fn()
vi.mock('../../api/admin', () => ({
  listAuditLogs: (...args: unknown[]) => listAuditLogsMock(...args),
}))

// --- Fixtures --------------------------------------------------------------

const NOW_UNIX = 1_725_000_000 // deterministic timestamp

function makeRow(overrides: Partial<AuditLog> = {}): AuditLog {
  return {
    id: 'row-1',
    action: 'invite.create',
    target_type: 'invite_code',
    target_id: 'E2E-ABCD',
    details: '{"code":"E2E-ABCD"}',
    performed_at: NOW_UNIX,
    ...overrides,
  }
}

// --- Helpers ---------------------------------------------------------------

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      {createElement(AdminAuditLogsPage)}
    </QueryClientProvider>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminAuditLogsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders loading skeleton rows while fetching', () => {
    // Never-resolving promise keeps the query in loading state.
    listAuditLogsMock.mockReturnValue(new Promise(() => {}))

    const { container } = renderPage()

    // 6 skeleton rows with pulse animation
    expect(container.querySelectorAll('.animate-pulse')).toHaveLength(6)
  })

  it('renders empty state when there are no logs', async () => {
    listAuditLogsMock.mockResolvedValue([])

    const { findByText } = renderPage()

    expect(await findByText(/no audit logs/i)).toBeInTheDocument()
  })

  it('renders rows from mocked listAuditLogs', async () => {
    const rows = [
      makeRow({ id: 'r1', action: 'invite.create', target_id: 'CODE-1' }),
      makeRow({ id: 'r2', action: 'user.disable', target_id: 'user-2' }),
    ]
    listAuditLogsMock.mockResolvedValue(rows)

    const { findByText, container } = renderPage()

    // Await row-specific content (the filter <option>s render immediately,
    // so awaiting the action name alone would resolve before rows paint).
    expect(await findByText('CODE-1')).toBeInTheDocument()
    expect(container.textContent).toContain('user.disable')
    expect(container.textContent).toContain('user-2')
    expect(container.textContent).toContain('invite.create')
  })

  it('formats performed_at as a human timestamp, not a raw Unix number', async () => {
    // Pick a value whose raw integer form should NOT appear verbatim.
    const row = makeRow({ performed_at: NOW_UNIX })
    listAuditLogsMock.mockResolvedValue([row])

    const { findByText, container } = renderPage()

    // Formatted string should appear…
    const formatted = await findByText(/2024-/)
    expect(formatted).toBeInTheDocument()
    // …and the raw integer should NOT be rendered as text anywhere.
    // (dayjs.unix renders "YYYY-MM-DD HH:mm:ss", never a bare epoch.)
    expect(container.textContent).not.toContain(String(NOW_UNIX))
  })

  it('changes the action filter and resets to page 1', async () => {
    listAuditLogsMock.mockResolvedValue([])

    const { findByLabelText } = renderPage()

    const select = await findByLabelText(/action/i)

    // Initially called with no action filter and page 1.
    await waitFor(() => {
      expect(listAuditLogsMock).toHaveBeenCalledWith({
        page: 1,
        limit: 20,
        action: undefined,
      })
    })

    // Change the filter to invite.delete.
    fireEvent.change(select, { target: { value: 'invite.delete' } })

    await waitFor(() => {
      expect(listAuditLogsMock).toHaveBeenCalledWith({
        page: 1,
        limit: 20,
        action: 'invite.delete',
      })
    })
  })

  it('disables Prev on the first page and enables Next when the page is full', async () => {
    // A full page (PAGE_SIZE = 20 rows) signals "hasMore".
    const fullPage = Array.from({ length: 20 }).map((_, i) =>
      makeRow({ id: `r${i}` }),
    )
    listAuditLogsMock.mockResolvedValue(fullPage)

    const { findAllByText } = renderPage()

    const prevButtons = await findAllByText(/prev/i)
    const nextButtons = await findAllByText(/next/i)
    const prev = prevButtons[0] as HTMLButtonElement
    const next = nextButtons[0] as HTMLButtonElement

    expect(prev.disabled).toBe(true)
    expect(next.disabled).toBe(false)
  })

  it('advances to page 2 and requests it with the new page number', async () => {
    const fullPage = Array.from({ length: 20 }).map((_, i) =>
      makeRow({ id: `r${i}` }),
    )
    listAuditLogsMock.mockResolvedValue(fullPage)

    const { findAllByText, findByText } = renderPage()

    // Wait for first page render.
    await findAllByText(/prev/i)

    const next = (await findAllByText(/next/i))[0] as HTMLButtonElement
    fireEvent.click(next)

    await findByText(/page 2/i)
    await waitFor(() => {
      expect(listAuditLogsMock).toHaveBeenCalledWith(
        expect.objectContaining({ page: 2 }),
      )
    })
  })

  it('enables Prev after advancing and disables Next on a partial page', async () => {
    // Return full pages first, then a partial page on page 2.
    const fullPage = Array.from({ length: 20 }).map((_, i) =>
      makeRow({ id: `r${i}` }),
    )
    listAuditLogsMock.mockResolvedValueOnce(fullPage).mockResolvedValueOnce([
      makeRow({ id: 'only-one' }),
    ])

    const { findAllByText, findByText } = renderPage()

    await findAllByText(/prev/i)
    const next1 = (await findAllByText(/next/i))[0] as HTMLButtonElement
    fireEvent.click(next1)

    // On page 2: Prev enabled, Next disabled (partial page).
    const prev2 = (await findAllByText(/prev/i))[0] as HTMLButtonElement
    const next2 = (await findAllByText(/next/i))[0] as HTMLButtonElement
    await findByText(/page 2/i)

    expect(prev2.disabled).toBe(false)
    expect(next2.disabled).toBe(true)
  })

  it('shows error message and retry button when fetch fails', async () => {
    listAuditLogsMock.mockRejectedValue(new Error('Network error'))

    const { findByText, getByText } = renderPage()

    expect(await findByText('Network error')).toBeInTheDocument()
    expect(getByText('Retry')).toBeInTheDocument()
  })

  it('is read-only — exposes no create / edit / delete / save buttons', async () => {
    listAuditLogsMock.mockResolvedValue([makeRow()])

    const { container, findByText } = renderPage()

    await findByText('invite.create')

    // The action filter <select> legitimately lists action names that
    // contain "create"/"delete"/"update" substrings, so a full-text
    // substring check would be too broad. Instead assert that no
    // <button> advertises a write affordance: the only buttons on the
    // page must be Prev / Next / Retry.
    const buttons = Array.from(container.querySelectorAll('button'))
    const labels = buttons.map((b) => (b.textContent ?? '').trim().toLowerCase())
    const writeLabels = labels.filter((l) =>
      /create|edit|delete|save|submit|reset|disable/.test(l),
    )
    expect(writeLabels).toEqual([])

    // No form and no submit button — there is nothing to POST.
    expect(container.querySelector('button[type="submit"]')).toBeNull()
    expect(container.querySelector('form')).toBeNull()
  })

  it('renders details column with the raw payload when present', async () => {
    const row = makeRow({ details: '{"code":"E2E-XYZ"}' })
    listAuditLogsMock.mockResolvedValue([row])

    const { findByText } = renderPage()

    // JSON payload surfaces in a <pre> (pretty-printed).
    const pre = await findByText(/"code": "E2E-XYZ"/)
    expect(pre).toBeInTheDocument()
  })

  it('renders an em dash for null details and missing target fields', async () => {
    // Use an action NOT present in the filter <option> list so findByText
    // can only resolve once the row cell paints (not the dropdown).
    const row = makeRow({
      action: 'test.special',
      details: null,
      target_type: null,
      target_id: null,
    })
    listAuditLogsMock.mockResolvedValue([row])

    const { findByText, container } = renderPage()

    await findByText('test.special')

    // target_type, target_id, and details all render "—" for null.
    const emDashes = Array.from(container.querySelectorAll('td')).filter(
      (td) => (td.textContent ?? '').trim() === '—',
    )
    expect(emDashes.length).toBeGreaterThanOrEqual(3)
  })
})
