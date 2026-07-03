import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import { createElement } from 'react'
import { VoteBuilderModal } from './VoteBuilderModal'

function renderModal(props: Partial<Parameters<typeof VoteBuilderModal>[0]> = {}) {
  const onClose = vi.fn()
  const onConfirm = vi.fn()
  const result = render(
    createElement(VoteBuilderModal, {
      isOpen: true,
      onClose,
      onConfirm,
      initialTitle: '',
      ...props,
    }),
  )
  return { ...result, onClose, onConfirm }
}

describe('VoteBuilderModal', () => {
  beforeEach(() => vi.clearAllMocks())

  afterEach(() => vi.restoreAllMocks())

  it('returns null when isOpen is false', () => {
    const { container } = renderModal({ isOpen: false })
    expect(container.firstChild).toBeNull()
  })

  it('renders with initial title pre-filled', () => {
    const { getByDisplayValue } = renderModal({ initialTitle: '中午吃啥' })
    expect(getByDisplayValue('中午吃啥')).toBeTruthy()
  })

  it('can type in title field', () => {
    const { getByPlaceholderText, getByDisplayValue } = renderModal({ initialTitle: '' })
    const input = getByPlaceholderText('投票标题') as HTMLInputElement
    fireEvent.change(input, { target: { value: '今晚吃啥' } })
    expect(getByDisplayValue('今晚吃啥')).toBeTruthy()
  })

  it('starts with 2 option inputs', () => {
    const { getAllByPlaceholderText } = renderModal()
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(2)
  })

  it('adds a new empty input when "添加选项" clicked', () => {
    const { getByText, getAllByPlaceholderText } = renderModal()
    fireEvent.click(getByText('添加选项'))
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(3)
  })

  it('disables "添加选项" at 10 options', () => {
    const { getByText, getAllByPlaceholderText } = renderModal()
    for (let i = 0; i < 8; i++) {
      fireEvent.click(getByText('添加选项'))
    }
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(10)
    const addBtn = getByText('添加选项').closest('button') as HTMLButtonElement
    expect(addBtn.disabled).toBe(true)
  })

  it('shows no × remove buttons at min (2) options', () => {
    const { queryAllByRole } = renderModal()
    const removeButtons = queryAllByRole('button', { name: /移除选项/ })
    expect(removeButtons).toHaveLength(0)
  })

  it('removes an option when × clicked and enforces min 2', () => {
    const { getByText, getAllByPlaceholderText, getAllByRole, queryAllByRole } = renderModal()
    fireEvent.click(getByText('添加选项'))
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(3)

    const removeButtons = getAllByRole('button', { name: /移除选项/ })
    expect(removeButtons).toHaveLength(3)
    fireEvent.click(removeButtons[0])
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(2)
    expect(queryAllByRole('button', { name: /移除选项/ })).toHaveLength(0)
  })

  it('disables 确认 when title is empty', () => {
    const { getByText, getAllByPlaceholderText } = renderModal({ initialTitle: '' })
    const inputs = getAllByPlaceholderText(/选项/)
    fireEvent.change(inputs[0], { target: { value: 'A' } })
    fireEvent.change(inputs[1], { target: { value: 'B' } })
    const confirmBtn = getByText('确认').closest('button') as HTMLButtonElement
    expect(confirmBtn.disabled).toBe(true)
  })

  it('disables 确认 when fewer than 2 options have text', () => {
    const { getByText, getAllByPlaceholderText } = renderModal({ initialTitle: '测试' })
    const inputs = getAllByPlaceholderText(/选项/)
    fireEvent.change(inputs[0], { target: { value: 'A' } })
    const confirmBtn = getByText('确认').closest('button') as HTMLButtonElement
    expect(confirmBtn.disabled).toBe(true)
  })

  it('disables 确认 when any option is whitespace', () => {
    const { getByText, getAllByPlaceholderText } = renderModal({ initialTitle: '测试' })
    const inputs = getAllByPlaceholderText(/选项/)
    fireEvent.change(inputs[0], { target: { value: 'A' } })
    fireEvent.change(inputs[1], { target: { value: '   ' } })
    const confirmBtn = getByText('确认').closest('button') as HTMLButtonElement
    expect(confirmBtn.disabled).toBe(true)
  })

  it('calls onConfirm with trimmed title + options when 确认 clicked', () => {
    const { getByText, getAllByPlaceholderText, onConfirm } = renderModal({ initialTitle: '  中午吃啥  ' })
    const inputs = getAllByPlaceholderText(/选项/)
    fireEvent.change(inputs[0], { target: { value: ' 麦当劳 ' } })
    fireEvent.change(inputs[1], { target: { value: '肯德基' } })
    fireEvent.click(getByText('确认'))
    expect(onConfirm).toHaveBeenCalledTimes(1)
    expect(onConfirm).toHaveBeenCalledWith('中午吃啥', ['麦当劳', '肯德基'])
  })

  it('calls onClose when 取消 clicked', () => {
    const { getByText, onClose } = renderModal()
    fireEvent.click(getByText('取消'))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('resets state when reopened', () => {
    const { rerender, getByDisplayValue, getAllByPlaceholderText, getByText } = renderModal({
      initialTitle: 'A',
    })
    fireEvent.change(getByDisplayValue('A'), { target: { value: 'B' } })
    fireEvent.click(getByText('添加选项'))
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(3)

    rerender(
      createElement(VoteBuilderModal, {
        isOpen: false,
        onClose: () => {},
        onConfirm: () => {},
        initialTitle: 'A',
      }),
    )
    rerender(
      createElement(VoteBuilderModal, {
        isOpen: true,
        onClose: () => {},
        onConfirm: () => {},
        initialTitle: 'C',
      }),
    )
    expect(getByDisplayValue('C')).toBeTruthy()
    expect(getAllByPlaceholderText(/选项/)).toHaveLength(2)
  })
})
