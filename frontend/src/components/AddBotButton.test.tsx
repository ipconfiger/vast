import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent, cleanup } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { Bot } from '../api/admin'

const useAddBotMock = vi.fn()
const addBotMutateMock = vi.fn()
const listBotsMock = vi.fn()

vi.mock('../api/permissions', () => ({
  useAddBot: () => useAddBotMock(),
}))

vi.mock('../api/admin', () => ({
  listBots: (...args: unknown[]) => listBotsMock(...args),
}))

vi.mock('../api/client', () => ({
  ApiClientError: class ApiClientError extends Error {
    code: string
    status: number
    constructor(code: string, message: string, status: number) {
      super(message)
      this.code = code
      this.status = status
      this.name = 'ApiClientError'
    }
  },
}))

const adminState: { adminToken: string | null } = { adminToken: null }
vi.mock('../stores/adminAuthStore', () => ({
  useAdminAuthStore: (selector: (s: { adminToken: string | null }) => unknown) =>
    selector(adminState),
}))

import { AddBotButton } from './AddBotButton'

function makeAddBot(returnValue?: Partial<ReturnType<typeof useAddBotMock>>) {
  return {
    mutate: (...args: Parameters<typeof addBotMutateMock>) =>
      addBotMutateMock(...args),
    mutateAsync: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
    isSuccess: false,
    ...returnValue,
  }
}

function makeBot(overrides: Partial<Bot> = {}): Bot {
  return {
    id: 'bot-1',
    user_id: 'u-bot-1',
    name: 'hermes',
    display_name: 'Hermes',
    api_url: 'http://localhost:8080',
    system_prompt: '',
    model: 'hermes',
    is_active: true,
    created_at: 0,
    ...overrides,
  }
}

function wrap(ui: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    createElement(QueryClientProvider, { client: queryClient }, ui),
  )
}

function renderButton(props: {
  channelId?: string
  memberUserIds?: Set<string>
}) {
  return wrap(
    createElement(AddBotButton, {
      channelId: props.channelId ?? 'ch-1',
      memberUserIds: props.memberUserIds ?? new Set<string>(),
    }),
  )
}

describe('AddBotButton', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    adminState.adminToken = null
    useAddBotMock.mockReturnValue(makeAddBot())
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  describe('collapsed state', () => {
    it('renders an Add Bot button', () => {
      const { getByText } = renderButton({})
      expect(getByText('Add Bot')).toBeTruthy()
    })

    it('expands the form on click', () => {
      const { getByText, queryByLabelText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      // No admin auth → manual input visible
      expect(queryByLabelText('Bot ID')).toBeTruthy()
    })
  })

  describe('without admin auth (manual input fallback)', () => {
    it('shows a text input for bot id', () => {
      const { getByText, getByLabelText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      expect(getByLabelText('Bot ID')).toBeTruthy()
    })

    it('disables Add Bot submit when input is empty', () => {
      const { getByText, getAllByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      const submit = getAllByText('Add Bot').pop()!.closest('button')!
      expect(submit.disabled).toBe(true)
    })

    it('enables submit when bot id is typed', () => {
      const { getByText, getByLabelText, getAllByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      fireEvent.change(getByLabelText('Bot ID'), {
        target: { value: '  bot-uuid-1  ' },
      })
      const submit = getAllByText('Add Bot').pop()!.closest('button')!
      expect(submit.disabled).toBe(false)
    })

    it('calls mutate with trimmed bot id and channelId on submit', () => {
      const { getByText, getByLabelText, getAllByText } = renderButton({
        channelId: 'ch-42',
      })
      fireEvent.click(getByText('Add Bot'))
      fireEvent.change(getByLabelText('Bot ID'), {
        target: { value: '  bot-uuid-1  ' },
      })
      fireEvent.click(getAllByText('Add Bot').pop()!)
      expect(addBotMutateMock).toHaveBeenCalledTimes(1)
      const [args] = addBotMutateMock.mock.calls[0]
      expect(args).toEqual({ channelId: 'ch-42', botId: 'bot-uuid-1' })
    })

    it('does not call fetch directly (uses apiClient abstraction)', () => {
      const fetchSpy = vi.fn()
      vi.stubGlobal('fetch', fetchSpy)
      const { getByText, getByLabelText, getAllByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      fireEvent.change(getByLabelText('Bot ID'), {
        target: { value: 'bot-1' },
      })
      fireEvent.click(getAllByText('Add Bot').pop()!)
      expect(fetchSpy).not.toHaveBeenCalled()
    })

    it('shows inline error message on 409 (already member)', () => {
      const apiErr = new (
        class extends Error {
          code = 'CONFLICT'
          status = 409
          name = 'ApiClientError'
        }
      )('Bot is already a member')
      useAddBotMock.mockReturnValue(
        makeAddBot({ isError: true, error: apiErr }),
      )
      const { getByText, getByRole } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      expect(getByRole('alert').textContent).toContain(
        'already a member',
      )
    })
  })

  describe('with admin auth (dropdown)', () => {
    beforeEach(() => {
      adminState.adminToken = 'admin-token'
    })

    it('fetches bot list via listBots when expanded', () => {
      listBotsMock.mockResolvedValue([])
      const { getByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      expect(listBotsMock).toHaveBeenCalledTimes(1)
    })

    it('renders available bots as dropdown options (excludes inactive)', async () => {
      const bots = [
        makeBot({ id: 'b1', name: 'hermes', user_id: 'u-b1' }),
        makeBot({
          id: 'b2',
          name: 'oracle',
          user_id: 'u-b2',
          is_active: false,
        }),
      ]
      listBotsMock.mockResolvedValue(bots)
      const { getByText, findByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      expect(await findByText('Select a bot...')).toBeTruthy()
      expect(await findByText(/@hermes/)).toBeTruthy()
      // inactive bot should not appear
      const oracleOpts = document.querySelectorAll('option')
      const oracle = Array.from(oracleOpts).find((o) =>
        o.textContent?.includes('oracle'),
      )
      expect(oracle).toBeUndefined()
    })

    it('excludes bots already in channel (by user_id)', async () => {
      const bots = [
        makeBot({ id: 'b1', name: 'hermes', user_id: 'u-b1' }),
        makeBot({ id: 'b2', name: 'oracle', user_id: 'u-b2' }),
      ]
      listBotsMock.mockResolvedValue(bots)
      const { getByText, findByText } = renderButton({
        memberUserIds: new Set(['u-b1']),
      })
      fireEvent.click(getByText('Add Bot'))
      // hermes is excluded; oracle should be present
      expect(await findByText(/@oracle/)).toBeTruthy()
      const hermesOpt = Array.from(document.querySelectorAll('option')).find(
        (o) => o.textContent?.includes('@hermes'),
      )
      expect(hermesOpt).toBeUndefined()
    })

    it('shows empty-state message when no bots available', async () => {
      listBotsMock.mockResolvedValue([])
      const { getByText, findByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      expect(await findByText('No available bots to add.')).toBeTruthy()
    })

    it('calls mutate with selected bot id on submit', async () => {
      const bots = [
        makeBot({ id: 'bot-uuid-1', name: 'hermes', user_id: 'u-b1' }),
      ]
      listBotsMock.mockResolvedValue(bots)
      const { getByText, findByText, getAllByText } = renderButton({
        channelId: 'ch-x',
      })
      fireEvent.click(getByText('Add Bot'))
      const hermesOpt = await findByText(/@hermes/)
      fireEvent.change(
        hermesOpt.closest('select')!,
        { target: { value: 'bot-uuid-1' } },
      )
      fireEvent.click(getAllByText('Add Bot').pop()!)
      expect(addBotMutateMock).toHaveBeenCalledTimes(1)
      const [args] = addBotMutateMock.mock.calls[0]
      expect(args).toEqual({ channelId: 'ch-x', botId: 'bot-uuid-1' })
    })

    it('shows error message when admin bot list fetch fails', async () => {
      listBotsMock.mockRejectedValue(new Error('Network'))
      const { getByText, findByText } = renderButton({})
      fireEvent.click(getByText('Add Bot'))
      expect(await findByText(/Failed to load bots/)).toBeTruthy()
    })
  })

  describe('non-owner visibility', () => {
    // Visibility is gated by the parent (MemberList); this test asserts
    // the component renders correctly when used, not that it self-hides.
    it('renders the Add Bot button regardless (parent gates visibility)', () => {
      const { getByText } = renderButton({})
      expect(getByText('Add Bot')).toBeTruthy()
    })
  })
})
