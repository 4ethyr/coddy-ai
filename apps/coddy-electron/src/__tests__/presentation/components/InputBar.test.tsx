// __tests__/presentation/components/InputBar.test.tsx
import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { InputBar } from '@/presentation/components/InputBar'

describe('InputBar', () => {
  it('renders textarea with placeholder', () => {
    render(<InputBar onSend={() => {}} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    expect(textarea).toBeInTheDocument()
    expect(tagName(textarea)).toBe('TEXTAREA')
    expect(textarea).not.toHaveFocus()
  })

  it('calls onSend with trimmed text on Enter', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, 'hello world{Enter}')

    expect(onSend).toHaveBeenCalledTimes(1)
    expect(onSend).toHaveBeenCalledWith('hello world')
  })

  it('does not call onSend on Shift+Enter (newline only)', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, 'line1{Shift>}{Enter}{/Shift}line2')

    // Type events still fire, but Enter+Shift is NOT a submit
    expect(onSend).not.toHaveBeenCalled()

    // Textarea should contain both lines (user typed Shift+Enter for newline)
    const el = textarea as HTMLTextAreaElement
    expect(el.value).toContain('line1')
    expect(el.value).toContain('line2')
  })

  it('disables textarea when disabled prop is true', () => {
    render(<InputBar onSend={() => {}} disabled />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    expect(textarea).toBeDisabled()
  })

  it('clears text after successful submit', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, 'submit me{Enter}')

    const el = textarea as HTMLTextAreaElement
    expect(el.value).toBe('')
  })

  it('does not submit empty text', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, '   {Enter}')

    expect(onSend).not.toHaveBeenCalled()
  })

  it('pastes clipboard text with Ctrl+Shift+V at the caret', async () => {
    const readText = vi.fn().mockResolvedValue(' pasted content ')
    Object.assign(navigator, {
      clipboard: { readText },
    })

    render(<InputBar onSend={() => {}} />)

    const textarea = screen.getByPlaceholderText(
      'Enter command or prompt...',
    ) as HTMLTextAreaElement
    await userEvent.type(textarea, 'hello')
    textarea.setSelectionRange(5, 5)

    fireEvent.keyDown(textarea, {
      key: 'v',
      code: 'KeyV',
      ctrlKey: true,
      shiftKey: true,
    })

    await waitFor(() => {
      expect(textarea.value).toBe('hello pasted content ')
    })
    expect(readText).toHaveBeenCalledOnce()
  })

  it('suggests slash workflow commands and inserts the selected command', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText(
      'Enter command or prompt...',
    ) as HTMLTextAreaElement
    await userEvent.type(textarea, '/pl')

    const planOption = screen.getByRole('option', {
      name: /\/plan plan a coding task/i,
    })
    await userEvent.click(planOption)

    expect(textarea.value).toBe('/plan ')
    expect(onSend).not.toHaveBeenCalled()
  })

  it('selects the active slash suggestion with Enter before submitting', async () => {
    const onSend = vi.fn()
    const user = userEvent.setup()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText(
      'Enter command or prompt...',
    ) as HTMLTextAreaElement
    await user.type(textarea, '/rev')
    await user.keyboard('{Enter}')

    expect(textarea.value).toBe('/review ')
    expect(onSend).not.toHaveBeenCalled()

    await user.type(textarea, 'recent diff{Enter}')

    expect(onSend).toHaveBeenCalledWith('/review recent diff')
  })

  it('closes slash suggestions with Escape without clearing the input', async () => {
    const user = userEvent.setup()
    render(<InputBar onSend={() => {}} />)

    const textarea = screen.getByPlaceholderText(
      'Enter command or prompt...',
    ) as HTMLTextAreaElement
    await user.type(textarea, '/')

    expect(
      screen.getByRole('listbox', { name: 'Slash commands' }),
    ).toBeInTheDocument()

    await user.keyboard('{Escape}')

    expect(textarea.value).toBe('/')
    expect(
      screen.queryByRole('listbox', { name: 'Slash commands' }),
    ).not.toBeInTheDocument()
  })
})

function tagName(el: HTMLElement): string {
  return el.tagName
}
