import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, waitFor, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createElement } from 'react'
import AdminBotsPage from './AdminBotsPage'
import type { Bot } from '../../api/admin'

// --- Mocks -----------------------------------------------------------------

const listBotsMock = vi.fn()
const createBotMock = vi.fn()
const updateBotMock = vi.fn()
const deleteBotMock = vi.fn()
const testBotMock = vi.fn()

vi.mock('../../api/admin', () => ({
  listBots: () => listBotsMock(),
  createBot: (arg: unknown) => createBotMock(arg),
  updateBot: (id: unknown, body: unknown) => updateBotMock(id, body),
  deleteBot: (id: unknown) => deleteBotMock(id),
  testBot: (id: unknown) => testBotMock(id),
  AdminApiClientError: class AdminApiClientError extends Error {
    code: string
    status: number
    constructor(code: string, message: string, status: number) {
      super(message)
      this.code = code
      this.status = status
      this.name = 'AdminApiClientError'
    }
  },
}))

const toastSuccessMock = vi.fn()
const toastErrorMock = vi.fn()
vi.mock('../../stores/toastStore', () => ({
  toast: {
    success: (msg: string) => toastSuccessMock(msg),
    error: (msg: string) => toastErrorMock(msg),
    info: vi.fn(),
    warning: vi.fn(),
  },
  useToastStore: () => ({ toasts: [] }),
}))

// --- Fixtures --------------------------------------------------------------

function makeBot(over: Partial<Bot> = {}): Bot {
  return {
    id: 'bot-1',
    user_id: 'u-bot-1',
    name: 'hermes',
    display_name: 'Hermes Assistant',
    api_url: 'https://hermes.example.com',
    system_prompt: 'You are a helpful assistant.',
    model: 'hermes',
    is_active: true,
    created_at: 1730000000,
    ...over,
  }
}

// --- Helpers ---------------------------------------------------------------

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      {createElement(AdminBotsPage)}
    </QueryClientProvider>,
  )
}

// --- Tests -----------------------------------------------------------------

describe('AdminBotsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listBotsMock.mockResolvedValue([makeBot(), makeBot({ id: 'bot-2', name: 'wiki', display_name: 'Wiki Bot' })])
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders loading spinner while fetching', () => {
    listBotsMock.mockReturnValue(new Promise(() => {}))
    const { container } = renderPage()
    expect(container.querySelector('.animate-spin')).toBeTruthy()
  })

  it('renders table rows when bots are loaded', async () => {
    const { findByText } = renderPage()
    expect(await findByText('hermes')).toBeInTheDocument()
    expect(await findByText('wiki')).toBeInTheDocument()
  })

  it('shows empty state when there are no bots', async () => {
    listBotsMock.mockResolvedValue([])
    const { findByText } = renderPage()
    expect(await findByText('No bots yet')).toBeInTheDocument()
  })

  it('shows error state when list fetch fails', async () => {
    listBotsMock.mockRejectedValue(new Error('Network down'))
    const { findByText } = renderPage()
    expect(await findByText('Network down')).toBeInTheDocument()
  })

  it('renders Active and Inactive status badges', async () => {
    listBotsMock.mockResolvedValue([
      makeBot({ is_active: true }),
      makeBot({ id: 'bot-2', name: 'wiki', is_active: false }),
    ])
    const { findAllByText } = renderPage()
    expect(await findAllByText('Active')).toHaveLength(1)
    expect(await findAllByText('Inactive')).toHaveLength(1)
  })

  it('opens create modal when "New Bot" is clicked', async () => {
    const { findByText, container } = renderPage()
    fireEvent.click(await findByText('New Bot'))
    expect(container.querySelector('#bot-name')).toBeTruthy()
    expect(container.querySelector('#bot-api-url')).toBeTruthy()
    expect(container.querySelector('#bot-api-key')).toBeTruthy()
    expect(container.querySelector('#bot-system-prompt')).toBeTruthy()
    expect(container.querySelector('#bot-model')).toBeTruthy()
  })

  it('creates a bot via modal and refreshes the list', async () => {
    const created = makeBot({ id: 'bot-new', name: 'newbot' })
    createBotMock.mockResolvedValue(created)
    let callCount = 0
    listBotsMock.mockImplementation(() => {
      callCount += 1
      return Promise.resolve(
        callCount === 1 ? [makeBot()] : [makeBot(), created],
      )
    })

    const { findByText, container } = renderPage()
    fireEvent.click(await findByText('New Bot'))

    fireEvent.change(container.querySelector('#bot-name') as HTMLInputElement, {
      target: { value: 'newbot' },
    })
    fireEvent.change(
      container.querySelector('#bot-api-url') as HTMLInputElement,
      { target: { value: 'https://new.example.com' } },
    )
    fireEvent.change(
      container.querySelector('#bot-api-key') as HTMLInputElement,
      { target: { value: 'sk-secret' } },
    )
    fireEvent.click(container.querySelector('#bot-submit') as HTMLButtonElement)

    await waitFor(() => {
      expect(createBotMock).toHaveBeenCalledWith({
        name: 'newbot',
        display_name: '',
        api_url: 'https://new.example.com',
        api_key: 'sk-secret',
        system_prompt: '',
        model: 'hermes',
      })
    })
    await waitFor(() => {
      expect(listBotsMock).toHaveBeenCalledTimes(2)
    })
  })

  it('opens edit modal with prefilled fields', async () => {
    const { findByLabelText, container } = renderPage()
    fireEvent.click(await findByLabelText('Edit hermes'))

    await waitFor(() => {
      expect(
        (container.querySelector('#bot-name') as HTMLInputElement).value,
      ).toBe('hermes')
    })
    expect(
      (container.querySelector('#bot-api-url') as HTMLInputElement).value,
    ).toBe('https://hermes.example.com')
    expect(
      (container.querySelector('#bot-system-prompt') as HTMLTextAreaElement)
        .value,
    ).toBe('You are a helpful assistant.')
    expect(
      (container.querySelector('#bot-model') as HTMLInputElement).value,
    ).toBe('hermes')
    // Name field is locked in edit mode.
    expect(
      (container.querySelector('#bot-name') as HTMLInputElement).disabled,
    ).toBe(true)
  })

  it('submits an edit without api_key when the field is left blank', async () => {
    updateBotMock.mockResolvedValue(makeBot({ display_name: 'Renamed' }))

    const { findByLabelText, container } = renderPage()
    fireEvent.click(await findByLabelText('Edit hermes'))

    fireEvent.change(
      container.querySelector('#bot-display-name') as HTMLInputElement,
      { target: { value: 'Renamed' } },
    )
    fireEvent.click(container.querySelector('#bot-submit') as HTMLButtonElement)

    await waitFor(() => {
      expect(updateBotMock).toHaveBeenCalledWith('bot-1', {
        display_name: 'Renamed',
        api_url: 'https://hermes.example.com',
        system_prompt: 'You are a helpful assistant.',
        model: 'hermes',
      })
    })
  })

  it('submits an edit with api_key when a new key is entered', async () => {
    updateBotMock.mockResolvedValue(makeBot())

    const { findByLabelText, container } = renderPage()
    fireEvent.click(await findByLabelText('Edit hermes'))

    fireEvent.change(
      container.querySelector('#bot-api-key') as HTMLInputElement,
      { target: { value: 'sk-rotated' } },
    )
    fireEvent.click(container.querySelector('#bot-submit') as HTMLButtonElement)

    await waitFor(() => {
      expect(updateBotMock).toHaveBeenCalledWith(
        'bot-1',
        expect.objectContaining({ api_key: 'sk-rotated' }),
      )
    })
  })

  it('toggles active state via the toggle action', async () => {
    updateBotMock.mockResolvedValue(makeBot({ is_active: false }))

    const { findByLabelText } = renderPage()
    fireEvent.click(await findByLabelText('Toggle hermes'))

    await waitFor(() => {
      expect(updateBotMock).toHaveBeenCalledWith('bot-1', {
        is_active: false,
      })
    })
  })

  it('opens a confirm dialog when delete is clicked', async () => {
    const { findByLabelText, findByText } = renderPage()
    fireEvent.click(await findByLabelText('Delete hermes'))
    expect(await findByText(/Are you sure/i)).toBeInTheDocument()
  })

  it('deletes the bot after confirming', async () => {
    deleteBotMock.mockResolvedValue(undefined)

    const { findByLabelText, findByText } = renderPage()
    fireEvent.click(await findByLabelText('Delete hermes'))
    fireEvent.click(await findByText('Delete'))

    await waitFor(() => {
      expect(deleteBotMock).toHaveBeenCalledWith('bot-1')
    })
  })

  it('renders a Test action button for each bot', async () => {
    const { findByLabelText } = renderPage()
    expect(await findByLabelText('Test hermes')).toBeInTheDocument()
  })

  it('calls testBot with the bot id when Test is clicked', async () => {
    testBotMock.mockResolvedValue({ ok: true, response: 'pong' })

    const { findByLabelText } = renderPage()
    fireEvent.click(await findByLabelText('Test hermes'))

    await waitFor(() => {
      expect(testBotMock).toHaveBeenCalledWith('bot-1')
    })
  })

  it('shows a success toast when testBot returns ok:true', async () => {
    testBotMock.mockResolvedValue({ ok: true, response: 'pong from hermes' })

    const { findByLabelText } = renderPage()
    fireEvent.click(await findByLabelText('Test hermes'))

    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalledWith(
        expect.stringContaining('✅ 连接成功'),
      )
    })
    expect(toastSuccessMock).toHaveBeenCalledWith(
      expect.stringContaining('pong from hermes'),
    )
    expect(toastErrorMock).not.toHaveBeenCalled()
  })

  it('shows an error toast when testBot returns ok:false', async () => {
    testBotMock.mockResolvedValue({ ok: false, error: 'Connection refused' })

    const { findByLabelText } = renderPage()
    fireEvent.click(await findByLabelText('Test hermes'))

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalledWith(
        expect.stringContaining('❌ 连接失败'),
      )
    })
    expect(toastErrorMock).toHaveBeenCalledWith(
      expect.stringContaining('Connection refused'),
    )
    expect(toastSuccessMock).not.toHaveBeenCalled()
  })
})
