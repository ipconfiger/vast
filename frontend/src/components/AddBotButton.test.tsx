import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

const useAddBotMock = vi.fn()
const usePublicBotsMock = vi.fn()

vi.mock('../api/permissions', () => ({
  useAddBot: () => useAddBotMock(),
}))

vi.mock('../api/channels', () => ({
  usePublicBots: () => usePublicBotsMock(),
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

import { AddBotButton } from './AddBotButton'

interface PublicBot {
  id: string
  name: string
  display_name: string
}

function makeAddBot(returnValue?: Partial<ReturnType<typeof useAddBotMock>>) {
  return {
    mutate: vi.fn(),
    mutateAsync: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
    isSuccess: false,
    ...returnValue,
  }
}

function makeBotsQuery(returnValue?: Partial<ReturnType<typeof usePublicBotsMock>>) {
  return {
    data: undefined,
    isLoading: false,
    isError: false,
    error: null,
    isPending: false,
    isFetched: true,
    ...returnValue,
  }
}

function makeBot(overrides: Partial<PublicBot> = {}): PublicBot {
  return {
    id: 'bot-1',
    name: 'hermes',
    display_name: 'Hermes',
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

function renderButton(props: { channelId?: string } = {}) {
  return wrap(
    createElement(AddBotButton, {
      channelId: props.channelId ?? 'ch-1',
    }),
  )
}

describe('AddBotButton', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAddBotMock.mockReturnValue(makeAddBot())
    usePublicBotsMock.mockReturnValue(makeBotsQuery())
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  describe('collapsed state', () => {
    it('renders an Add Bot button', () => {
      const { getByText } = renderButton()
      expect(getByText('Add Bot')).toBeTruthy()
    })

    it('expands to dropdown on click', () => {
      usePublicBotsMock.mockReturnValue(
        makeBotsQuery({ data: [makeBot()] }),
      )
      const { getByText, getByLabelText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      expect(getByLabelText('Select a bot')).toBeTruthy()
    })
  })

  describe('dropdown mode', () => {
    it('shows loading state while fetching bots', () => {
      usePublicBotsMock.mockReturnValue(makeBotsQuery({ isLoading: true }))
      const { getByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      expect(getByText('Loading bots...')).toBeTruthy()
    })

    it('shows empty-state message when no bots exist', () => {
      usePublicBotsMock.mockReturnValue(
        makeBotsQuery({ data: [] as PublicBot[] }),
      )
      const { getByText, findByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      return waitFor(() => {
        expect(findByText('No available bots to add.')).toBeTruthy()
      })
    })

    it('shows error message when bot list fetch fails', () => {
      usePublicBotsMock.mockReturnValue(
        makeBotsQuery({ isError: true, error: new Error('Network') }),
      )
      const { getByText, findByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      return waitFor(() => {
        expect(findByText('Failed to load bots.')).toBeTruthy()
      })
    })

    it('renders active bots as dropdown options', () => {
      const bots = [
        makeBot({ id: 'b1', name: 'hermes', display_name: 'Hermes' }),
        makeBot({ id: 'b2', name: 'oracle', display_name: 'Oracle' }),
      ]
      usePublicBotsMock.mockReturnValue(makeBotsQuery({ data: bots }))
      const { getByText, findByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      return waitFor(() => {
        expect(findByText(/@hermes/)).toBeTruthy()
        expect(findByText(/@oracle/)).toBeTruthy()
      })
    })

    it('disables submit when no bot is selected', () => {
      const bots = [makeBot()]
      usePublicBotsMock.mockReturnValue(makeBotsQuery({ data: bots }))
      const { getByText, getAllByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      const submit = getAllByText('Add Bot').pop()!.closest('button')!
      expect(submit.disabled).toBe(true)
    })

    it('enables submit when a bot is selected', () => {
      const bots = [makeBot({ id: 'bot-uuid-1' })]
      usePublicBotsMock.mockReturnValue(makeBotsQuery({ data: bots }))
      const { getByText, getByLabelText, getAllByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      fireEvent.change(getByLabelText('Select a bot'), {
        target: { value: 'bot-uuid-1' },
      })
      const submit = getAllByText('Add Bot').pop()!.closest('button')!
      expect(submit.disabled).toBe(false)
    })

    it('calls mutate with selected bot id and channelId on submit', () => {
      const mutateMock = vi.fn()
      useAddBotMock.mockReturnValue(makeAddBot({ mutate: mutateMock }))
      const bots = [makeBot({ id: 'bot-uuid-1' })]
      usePublicBotsMock.mockReturnValue(makeBotsQuery({ data: bots }))
      const { getByText, getByLabelText, getAllByText } = renderButton({
        channelId: 'ch-42',
      })
      fireEvent.click(getByText('Add Bot'))
      fireEvent.change(getByLabelText('Select a bot'), {
        target: { value: 'bot-uuid-1' },
      })
      fireEvent.click(getAllByText('Add Bot').pop()!)
      expect(mutateMock).toHaveBeenCalledTimes(1)
      const [args] = mutateMock.mock.calls[0]
      expect(args).toEqual({ channelId: 'ch-42', botId: 'bot-uuid-1' })
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
      const { getByText, getByRole } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      expect(getByRole('alert').textContent).toContain('already a member')
    })

    it('does not call fetch directly (uses apiClient abstraction)', () => {
      const fetchSpy = vi.fn()
      vi.stubGlobal('fetch', fetchSpy)
      const bots = [makeBot({ id: 'bot-1' })]
      usePublicBotsMock.mockReturnValue(makeBotsQuery({ data: bots }))
      const { getByText, getByLabelText, getAllByText } = renderButton()
      fireEvent.click(getByText('Add Bot'))
      fireEvent.change(getByLabelText('Select a bot'), {
        target: { value: 'bot-1' },
      })
      fireEvent.click(getAllByText('Add Bot').pop()!)
      expect(fetchSpy).not.toHaveBeenCalled()
    })
  })
})
