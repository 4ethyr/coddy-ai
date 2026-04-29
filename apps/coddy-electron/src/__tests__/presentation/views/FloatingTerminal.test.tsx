import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { FloatingTerminal } from '@/presentation/views/FloatingTerminal/FloatingTerminal'

const sessionContext = {
  session: {
    id: 'test-session',
    mode: 'FloatingTerminal',
    status: 'Idle',
    policy: 'Practice',
    selected_model: { provider: 'ollama', name: 'gemma4-E2B' },
    voice: { enabled: true, speaking: false, muted: false },
    screen_context: null,
    workspace_context: [],
    messages: [],
    active_run: null,
    streaming_text: '',
  },
  connecting: false,
  reconnecting: false,
  error: null,
  ask: vi.fn(),
  reconnect: vi.fn(),
  selectModel: vi.fn(),
  openUi: vi.fn(),
  captureVoice: vi.fn(),
  cancelVoiceCapture: vi.fn(),
  captureAndExplain: vi.fn(),
  dismissConfirmation: vi.fn(),
}

vi.mock('@/presentation/hooks', () => ({
  useSessionContext: () => sessionContext,
}))

describe('FloatingTerminal', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    Object.defineProperty(window, 'replApi', {
      configurable: true,
      value: {
        invoke: vi.fn().mockResolvedValue({ maximized: true }),
        on: vi.fn(),
      },
    })
  })

  it('keeps the terminal visually expanded after maximize is clicked', async () => {
    render(<FloatingTerminal />)

    const terminal = screen.getByRole('main')
    expect(terminal).toHaveClass('w-[calc(100vw-48px)]')

    await userEvent.click(screen.getByRole('button', { name: 'Maximize' }))

    expect(window.replApi.invoke).toHaveBeenCalledWith('window:maximize')
    expect(terminal).toHaveClass('w-screen')
    expect(terminal).toHaveClass('h-screen')
    expect(
      screen.getByRole('button', { name: 'Restore floating terminal' }),
    ).toBeInTheDocument()
  })

  it('keeps header controls in a dedicated layer above terminal output', () => {
    render(<FloatingTerminal />)

    expect(screen.getByTestId('floating-terminal-header')).toHaveClass(
      'floating-terminal-header',
    )
    expect(screen.getByTestId('floating-terminal-canvas')).toHaveClass(
      'terminal-canvas',
    )
  })
})
