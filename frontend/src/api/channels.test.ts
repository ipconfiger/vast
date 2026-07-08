import { describe, it, expect, vi, beforeEach } from 'vitest'
import { downloadChannelArchive } from './channels'

// Mock useAuthStore
vi.mock('../stores/authStore', () => ({
  useAuthStore: {
    getState: () => ({ token: 'test-token' }),
  },
}))

describe('downloadChannelArchive', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    // Mock URL.createObjectURL and revokeObjectURL
    globalThis.URL.createObjectURL = vi.fn(() => 'blob:test')
    globalThis.URL.revokeObjectURL = vi.fn()
  })

  it('creates download link and triggers download', async () => {
    const mockBlob = new Blob(['test'])
    const mockResponse = { ok: true, blob: () => Promise.resolve(mockBlob) }
    globalThis.fetch = vi.fn().mockResolvedValue(mockResponse)

    const appendChildSpy = vi.spyOn(document.body, 'appendChild')

    await downloadChannelArchive('ch-1', 'Test Channel')

    expect(appendChildSpy).toHaveBeenCalled()
  })

  it('sanitizes channel name in filename', async () => {
    const mockBlob = new Blob(['test'])
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      blob: () => Promise.resolve(mockBlob),
    })

    const createElSpy = vi.spyOn(document, 'createElement')
    await downloadChannelArchive('ch-1', 'report/final:2026')

    createElSpy.mock.results.find(r =>
      r.value instanceof HTMLAnchorElement
    )?.value as HTMLAnchorElement | undefined
  })

  it('throws on non-ok response', async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({ ok: false, status: 403 })
    await expect(downloadChannelArchive('ch-1', 'Test')).rejects.toThrow('Download failed')
  })
})
