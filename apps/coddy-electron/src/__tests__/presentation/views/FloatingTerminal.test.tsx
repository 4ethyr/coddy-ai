import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReplSession } from '@/domain'
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
    pending_permission: null,
    tool_activity: [],
    subagent_activity: [],
    streaming_text: '',
  } as ReplSession,
  connecting: false,
  reconnecting: false,
  error: null,
  ask: vi.fn(),
  reconnect: vi.fn(),
  selectModel: vi.fn(),
  listProviderModels: vi.fn(),
  openUi: vi.fn(),
  captureVoice: vi.fn(),
  cancelVoiceCapture: vi.fn(),
  captureAndExplain: vi.fn(),
  dismissConfirmation: vi.fn(),
  replyPermission: vi.fn(),
}

vi.mock('@/presentation/hooks', () => ({
  useSessionContext: () => sessionContext,
}))

describe('FloatingTerminal', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Idle',
      pending_permission: null,
      subagent_activity: [],
      streaming_text: '',
    }
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

  it('renders subagent lifecycle readiness in the activity area', () => {
    sessionContext.session = {
      ...sessionContext.session,
      subagent_activity: [
        {
          id: 'eval-runner:evaluation',
          name: 'eval-runner',
          mode: 'evaluation',
          status: 'Prepared',
          readiness_score: 100,
          reason: null,
        },
      ],
    }

    render(<FloatingTerminal />)

    expect(screen.getByText('agent.subagents')).toBeInTheDocument()
    expect(screen.getByText('eval-runner [evaluation]')).toBeInTheDocument()
    expect(screen.getByText('Prepared // 100')).toBeInTheDocument()
  })

  it('renders pending tool approval actions above the input', async () => {
    sessionContext.session = {
      ...sessionContext.session,
      status: 'AwaitingToolApproval',
      pending_permission: {
        id: 'perm-1',
        session_id: 'test-session',
        run_id: 'run-1',
        tool_call_id: 'call-1',
        tool_name: 'filesystem.apply_edit',
        permission: 'WriteWorkspace',
        patterns: ['src/App.tsx'],
        risk_level: 'High',
        metadata: { path: 'src/App.tsx' },
        requested_at_unix_ms: 1775000000000,
      },
    }

    render(<FloatingTerminal />)

    expect(screen.getByText('filesystem.apply_edit')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Once' }))

    expect(sessionContext.replyPermission).toHaveBeenCalledWith('perm-1', 'Once')
    expect(screen.getByPlaceholderText('Tool approval required')).toBeDisabled()
  })
})
