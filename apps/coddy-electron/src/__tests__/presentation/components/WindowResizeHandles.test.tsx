import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { WindowResizeHandles } from '@/presentation/components/WindowResizeHandles'

describe('WindowResizeHandles', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'replApi', {
      configurable: true,
      value: {
        invoke: vi.fn().mockResolvedValue({ ok: true }),
        on: vi.fn(),
      },
    })
  })

  it('streams resize drag coordinates through the window IPC bridge', () => {
    render(<WindowResizeHandles />)

    fireEvent(screen.getByTestId('window-resize-se'), pointerEvent(
      'pointerdown',
      100,
      120,
      0,
    ))
    fireEvent(window, pointerEvent('pointermove', 150, 170))
    fireEvent(window, pointerEvent('pointerup', 150, 170))

    expect(window.replApi.invoke).toHaveBeenCalledWith(
      'window:resize-start',
      { edge: 'se', screenX: 100, screenY: 120 },
    )
    expect(window.replApi.invoke).toHaveBeenCalledWith(
      'window:resize-drag',
      { screenX: 150, screenY: 170 },
    )
    expect(window.replApi.invoke).toHaveBeenCalledWith('window:resize-end')
  })
})

function pointerEvent(
  type: string,
  screenX: number,
  screenY: number,
  button = 0,
) {
  const event = new Event(type, { bubbles: true })
  Object.defineProperty(event, 'button', { value: button })
  Object.defineProperty(event, 'screenX', { value: screenX })
  Object.defineProperty(event, 'screenY', { value: screenY })
  Object.defineProperty(event, 'clientX', { value: screenX })
  Object.defineProperty(event, 'clientY', { value: screenY })
  return event
}
