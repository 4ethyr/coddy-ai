import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ConversationRecord, ReplSession, ReplToolCatalogItem } from '@/domain'
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
    agent_run: null,
    tool_activity: [],
    subagent_activity: [],
    streaming_text: '',
  } as ReplSession,
  connecting: false,
  reconnecting: false,
  error: null,
  activeWorkspacePath: null as string | null,
  toolCatalog: [] as ReplToolCatalogItem[],
  conversationHistory: [] as ConversationRecord[],
  conversationHistoryStatus: 'idle',
  conversationHistoryError: null,
  workspaceSelectionStatus: 'idle',
  workspaceSelectionError: null,
  ask: vi.fn(),
  newSession: vi.fn(),
  reconnect: vi.fn(),
  selectModel: vi.fn(),
  listProviderModels: vi.fn(),
  openUi: vi.fn(),
  captureVoice: vi.fn(),
  cancelVoiceCapture: vi.fn(),
  captureAndExplain: vi.fn(),
  dismissConfirmation: vi.fn(),
  replyPermission: vi.fn(),
  selectWorkspaceFolder: vi.fn(),
  loadConversationHistory: vi.fn(),
  openConversation: vi.fn(),
}

vi.mock('@/presentation/hooks', () => ({
  useSessionContext: () => sessionContext,
}))

describe('FloatingTerminal', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.localStorage.clear()
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Idle',
      pending_permission: null,
      subagent_activity: [],
      streaming_text: '',
    }
    sessionContext.conversationHistory = []
    sessionContext.activeWorkspacePath = null
    sessionContext.toolCatalog = []
    sessionContext.conversationHistoryStatus = 'idle'
    sessionContext.conversationHistoryError = null
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
    expect(screen.getByTestId('floating-terminal-canvas')).toHaveClass(
      'select-text',
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
          required_output_fields: ['score', 'passed'],
          output_additional_properties_allowed: false,
          reason: null,
        },
      ],
    }

    render(<FloatingTerminal />)

    expect(screen.getByText('agent.subagents')).toBeInTheDocument()
    expect(screen.getByText('eval-runner [evaluation]')).toBeInTheDocument()
    expect(screen.getByText('output: score, passed // strict')).toBeInTheDocument()
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

  it('offers copy selection from the right-click/two-finger context menu', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, {
      clipboard: { writeText },
    })
    const selection = {
      toString: () => 'selected snippet',
      rangeCount: 1,
      anchorNode: null,
    }
    vi.spyOn(window, 'getSelection').mockReturnValue(selection as unknown as Selection)

    sessionContext.session = {
      ...sessionContext.session,
      messages: [
        {
          id: 'assistant-1',
          role: 'assistant',
          text: 'selected snippet with more text',
        },
      ],
    }

    render(<FloatingTerminal />)

    fireEvent.contextMenu(screen.getByText(/selected snippet with more text/i), {
      clientX: 80,
      clientY: 96,
    })
    await userEvent.click(screen.getByRole('button', { name: 'Copy selection' }))

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('selected snippet')
    })
  })

  it('copies selected transcript text with Ctrl+Shift+C', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, {
      clipboard: { writeText },
    })
    const selection = {
      toString: () => 'shortcut selection',
      rangeCount: 1,
      anchorNode: null,
    }
    vi.spyOn(window, 'getSelection').mockReturnValue(selection as unknown as Selection)

    sessionContext.session = {
      ...sessionContext.session,
      messages: [
        {
          id: 'assistant-2',
          role: 'assistant',
          text: 'shortcut selection text',
        },
      ],
    }

    render(<FloatingTerminal />)

    fireEvent.keyDown(window, {
      key: 'c',
      code: 'KeyC',
      ctrlKey: true,
      shiftKey: true,
    })

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('shortcut selection')
    })
  })

  it('shows the Escape cancel hint while thinking', () => {
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Thinking',
      active_run: 'run-1',
    }

    render(<FloatingTerminal />)

    expect(screen.getByText('Pressione (Esc) para parar.')).toBeInTheDocument()
  })

  it('shows the Escape cancel hint while streaming', () => {
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Streaming',
      active_run: 'run-1',
      streaming_text: 'Partial response',
    }

    render(<FloatingTerminal />)

    expect(screen.getByText('Pressione (Esc) para parar.')).toBeInTheDocument()
  })

  it('renders streaming responses with markdown formatting', () => {
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Streaming',
      active_run: 'run-1',
      streaming_text:
        '### Principais funcionalidades:\n\n1. **Deteccao:** encontra problemas.\n\nUse *pipeline* de CI.',
    }

    const { container } = render(<FloatingTerminal />)

    expect(screen.getByText('Principais funcionalidades:')).toBeInTheDocument()
    expect(screen.getByText('Deteccao:')).toBeInTheDocument()
    expect(screen.getByText('pipeline')).toBeInTheDocument()
    expect(container.textContent).not.toContain('###')
    expect(container.textContent).not.toContain('**Deteccao:**')
    expect(container.textContent).not.toContain('*pipeline*')
  })

  it('opens desktop workspace from the /workspace slash command', async () => {
    render(<FloatingTerminal />)

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/workspace',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(window.localStorage.getItem('coddy:desktop-active-tab')).toBe(
      'workspace',
    )
    expect(sessionContext.openUi).toHaveBeenCalledWith('DesktopApp')
  })

  it('shows local session status from the /status slash command', async () => {
    sessionContext.activeWorkspacePath = '/home/user/project'
    render(<FloatingTerminal />)

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/status',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(sessionContext.openUi).not.toHaveBeenCalled()
    expect(
      screen.getByRole('region', { name: 'Session status' }),
    ).toBeInTheDocument()
    expect(screen.getByText('status=Idle')).toBeInTheDocument()
    expect(screen.getByText('model=ollama/gemma4-E2B')).toBeInTheDocument()
    expect(screen.getByText('workspace=/home/user/project')).toBeInTheDocument()
  })

  it('dispatches coding workflow slash commands as guarded prompts', async () => {
    render(<FloatingTerminal />)

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/plan improve tool reliability',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.openUi).not.toHaveBeenCalled()
    expect(sessionContext.ask).toHaveBeenCalledWith(
      expect.stringContaining('Plan-only coding workflow'),
    )
    expect(sessionContext.ask).toHaveBeenCalledWith(
      expect.stringContaining('Goal: improve tool reliability'),
    )
  })

  it('opens settings from the common /settins typo without sending a prompt', async () => {
    render(<FloatingTerminal />)

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/settins',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(
      screen.getByRole('dialog', { name: 'Terminal settings' }),
    ).toBeInTheDocument()
  })

  it('loads history and starts new sessions from slash commands', async () => {
    sessionContext.conversationHistory = [
      {
        summary: {
          session_id: 'session-1',
          title: 'Review Coddy runtime',
          created_at_unix_ms: 1,
          updated_at_unix_ms: 2,
          message_count: 2,
          selected_model: { provider: 'openrouter', name: 'deepseek' },
          mode: 'FloatingTerminal',
        },
        messages: [],
      },
    ]
    sessionContext.conversationHistoryStatus = 'succeeded'
    render(<FloatingTerminal />)

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/history',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.loadConversationHistory).toHaveBeenCalledOnce()
    expect(screen.getByText('Review Coddy runtime')).toBeInTheDocument()

    await userEvent.click(
      screen.getByRole('button', { name: /Review Coddy runtime/i }),
    )

    expect(sessionContext.openConversation).toHaveBeenCalledWith('session-1')

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/new',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.newSession).toHaveBeenCalledOnce()
  })

  it('persists /speak preference and passes it to voice capture', async () => {
    sessionContext.captureVoice.mockResolvedValue({ text: 'voice command' })
    render(<FloatingTerminal />)

    await userEvent.type(
      screen.getByPlaceholderText('Enter command or prompt...'),
      '/speak on',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))
    await userEvent.click(screen.getByRole('button', { name: 'Voice input' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(window.localStorage.getItem('coddy:settings')).toContain(
      '"speakVoiceResponses":true',
    )
    expect(sessionContext.captureVoice).toHaveBeenCalledWith({
      speakResponse: true,
    })
  })
})
