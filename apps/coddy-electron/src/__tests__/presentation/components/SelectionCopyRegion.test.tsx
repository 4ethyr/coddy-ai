import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SelectionCopyRegion } from '@/presentation/components/SelectionCopyRegion'

describe('SelectionCopyRegion', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    })
  })

  it('offers copy only for selections inside the region', async () => {
    render(
      <>
        <p data-testid="outside">outside selection</p>
        <SelectionCopyRegion>
          <p data-testid="inside">inside selection</p>
        </SelectionCopyRegion>
      </>,
    )
    mockSelection('inside selection', screen.getByTestId('inside').firstChild)

    fireEvent.contextMenu(screen.getByTestId('inside'), {
      clientX: 12,
      clientY: 18,
    })
    await userEvent.click(screen.getByRole('button', { name: 'Copy selection' }))

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        'inside selection',
      )
    })
  })

  it('does not offer the context menu for selections outside the region', () => {
    render(
      <>
        <p data-testid="outside">outside selection</p>
        <SelectionCopyRegion>
          <p data-testid="inside">inside selection</p>
        </SelectionCopyRegion>
      </>,
    )
    mockSelection('outside selection', screen.getByTestId('outside').firstChild)

    fireEvent.contextMenu(screen.getByTestId('inside'), {
      clientX: 12,
      clientY: 18,
    })

    expect(screen.queryByRole('button', { name: 'Copy selection' })).toBeNull()
  })

  it('ignores Ctrl+Shift+C for selections outside the region', async () => {
    render(
      <>
        <p data-testid="outside">outside selection</p>
        <SelectionCopyRegion>
          <p data-testid="inside">inside selection</p>
        </SelectionCopyRegion>
      </>,
    )
    mockSelection('outside selection', screen.getByTestId('outside').firstChild)

    fireEvent.keyDown(window, {
      key: 'c',
      code: 'KeyC',
      ctrlKey: true,
      shiftKey: true,
    })

    await waitFor(() => {
      expect(navigator.clipboard.writeText).not.toHaveBeenCalled()
    })
  })
})

function mockSelection(text: string, node: ChildNode | null): void {
  const selection = {
    toString: () => text,
    rangeCount: node ? 1 : 0,
    anchorNode: node,
    getRangeAt: () => ({
      commonAncestorContainer: node,
      intersectsNode: (candidate: Node) => Boolean(node && candidate.contains(node)),
    }),
  }
  vi.spyOn(window, 'getSelection').mockReturnValue(selection as unknown as Selection)
}
