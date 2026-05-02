import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ConversationRecord, ReplSession, ReplToolCatalogItem } from '@/domain'
import { DesktopApp } from '@/presentation/views/DesktopApp/DesktopApp'

const sessionContext = {
  session: {
    id: 'test-session',
    mode: 'DesktopApp',
    status: 'Idle',
    policy: 'Practice',
    selected_model: { provider: 'vertex', name: 'claude-sonnet-test' },
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
  toolCatalog: [] as ReplToolCatalogItem[],
  connecting: false,
  reconnecting: false,
  error: null,
  activeWorkspacePath: null as string | null,
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
  runMultiagentEval: vi.fn(),
  runPromptBatteryEval: vi.fn(),
  runQualityEval: vi.fn(),
  loadConversationHistory: vi.fn(),
  openConversation: vi.fn(),
}

vi.mock('@/presentation/hooks', () => ({
  useSessionContext: () => sessionContext,
}))

describe('DesktopApp', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.localStorage.clear()
    Reflect.deleteProperty(window, 'speechSynthesis')
    Reflect.deleteProperty(window, 'SpeechSynthesisUtterance')
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Idle',
      selected_model: { provider: 'vertex', name: 'claude-sonnet-test' },
      subagent_activity: [],
      tool_activity: [],
      pending_permission: null,
      streaming_text: '',
    }
    sessionContext.activeWorkspacePath = null
    sessionContext.toolCatalog = []
    sessionContext.conversationHistory = []
    sessionContext.conversationHistoryStatus = 'idle'
    sessionContext.conversationHistoryError = null
    sessionContext.workspaceSelectionStatus = 'idle'
    sessionContext.workspaceSelectionError = null
    sessionContext.listProviderModels.mockResolvedValue({
      provider: 'ollama',
      source: 'local',
      models: [],
      fetchedAtUnixMs: 1_775_000_000_000,
    })
    Object.defineProperty(window, 'replApi', {
      configurable: true,
      value: {
        invoke: vi.fn().mockResolvedValue({ maximized: true }),
        on: vi.fn(),
      },
    })
  })

  it('shows provider-aware runtime status on the model routing panel', async () => {
    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: /Neural_Link/ }))

    expect(screen.getAllByText('runtime ready').length).toBeGreaterThan(0)
    expect(
      screen.getByText(/Gemini API-key models execute through generateContent/i),
    ).toBeInTheDocument()
  })

  it('separates the selected chat model from the speech fallback route', async () => {
    sessionContext.session = {
      ...sessionContext.session,
      selected_model: {
        provider: 'openrouter',
        name: 'deepseek/deepseek-v4-flash',
      },
    }

    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: /Neural_Link/ }))

    expect(screen.getByText('SPEECH ROUTE')).toBeInTheDocument()
    expect(screen.getByText('Needs fallback')).toBeInTheDocument()
    expect(screen.getByText('Chat model')).toBeInTheDocument()
    expect(screen.getByText('Configured speech fallback')).toBeInTheDocument()
    expect(
      screen.getByText('chat model is text-only; use a dedicated TTS route'),
    ).toBeInTheDocument()
    expect(screen.queryByText('FALLBACK TTS')).not.toBeInTheDocument()
  })

  it('keeps desktop navigation above the model catalog popover', async () => {
    const { container } = render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: /Neural_Link/ }))
    await userEvent.click(
      screen.getByRole('button', {
        name: /Active model vertex\/claude-sonnet-test/i,
      }),
    )

    expect(container.querySelector('.desktop-sidebar')?.className).toContain(
      'z-[230]',
    )
    expect(screen.getByTestId('model-selector-popover').className).toContain(
      'z-[220]',
    )

    await userEvent.click(screen.getByRole('button', { name: /Terminal/ }))

    expect(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
    ).toBeInTheDocument()
  })

  it('renders model thinking controls in settings', async () => {
    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: 'Open config' }))

    expect(screen.getByText('Model thinking')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'deep' })).toBeInTheDocument()
    expect(screen.getByText('2048 tokens')).toBeInTheDocument()
  })

  it('lets the user choose the preferred local model provider in settings', async () => {
    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: 'Open config' }))
    await userEvent.click(screen.getByRole('button', { name: 'vllm' }))

    expect(screen.getByRole('button', { name: 'vllm' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(window.localStorage.getItem('coddy:settings')).toContain(
      '"providerPreference":"vllm"',
    )
  })

  it('renders voice command control in desktop mode', async () => {
    sessionContext.captureVoice.mockResolvedValue({ text: 'voice command' })

    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: 'Voice input' }))

    expect(sessionContext.captureVoice).toHaveBeenCalledWith({
      speakResponse: false,
    })
  })

  it('lets the user enable spoken voice responses from desktop settings', async () => {
    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: 'Open config' }))
    await userEvent.click(
      screen.getByRole('button', { name: 'Spoken responses off' }),
    )

    expect(screen.getByRole('button', { name: 'Spoken responses on' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(window.localStorage.getItem('coddy:settings')).toContain(
      '"speakVoiceResponses":true',
    )
  })

  it('uses browser speech fallback for spoken desktop voice responses', async () => {
    sessionContext.captureVoice.mockResolvedValue({ text: 'desktop voice answer' })
    const speak = vi.fn()
    Object.defineProperty(window, 'speechSynthesis', {
      configurable: true,
      value: { cancel: vi.fn(), speak },
    })
    Object.defineProperty(window, 'SpeechSynthesisUtterance', {
      configurable: true,
      value: class {
        constructor(public text: string) {}
      },
    })
    render(<DesktopApp />)

    await userEvent.click(screen.getByRole('button', { name: 'Open config' }))
    await userEvent.click(
      screen.getByRole('button', { name: 'Spoken responses off' }),
    )
    await userEvent.click(screen.getByRole('button', { name: 'Voice input' }))

    expect(sessionContext.captureVoice).toHaveBeenCalledWith({
      speakResponse: true,
    })
    expect(speak).toHaveBeenCalledOnce()
    expect(speak).toHaveBeenCalledWith(
      expect.objectContaining({ text: 'desktop voice answer' }),
    )
  })

  it('renders streaming responses with markdown formatting in desktop mode', () => {
    sessionContext.session = {
      ...sessionContext.session,
      status: 'Streaming',
      active_run: 'run-1',
      streaming_text:
        '### Principais funcionalidades:\n\n1. **Deteccao:** encontra problemas.\n\nUse *pipeline* de CI.',
    }

    const { container } = render(<DesktopApp />)

    expect(screen.getByText('Principais funcionalidades:')).toBeInTheDocument()
    expect(screen.getByText('Deteccao:')).toBeInTheDocument()
    expect(screen.getByText('pipeline')).toBeInTheDocument()
    expect(container.textContent).not.toContain('###')
    expect(container.textContent).not.toContain('**Deteccao:**')
    expect(container.textContent).not.toContain('*pipeline*')
  })

  it('renders subagent lifecycle readiness in the desktop activity panel', () => {
    sessionContext.session = {
      ...sessionContext.session,
      active_run: 'run-1',
      subagent_activity: [
        {
          id: 'security-reviewer:read-only',
          name: 'security-reviewer',
          mode: 'read-only',
          status: 'Blocked',
          readiness_score: 80,
          required_output_fields: ['riskLevel', 'findings'],
          output_additional_properties_allowed: false,
          reason: 'validation checklist is underspecified',
        },
      ],
    }

    render(<DesktopApp />)

    expect(
      screen.getByText(
        'subagent.security-reviewer // Blocked // readiness=80 // output=riskLevel, findings // strict',
      ),
    ).toBeInTheDocument()
  })

  it('routes /models from the desktop input without sending it to the model', async () => {
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/models',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(screen.getAllByText('runtime ready').length).toBeGreaterThan(0)
  })

  it('shows local session status from the /status slash command', async () => {
    sessionContext.activeWorkspacePath = '/home/user/coddy'
    sessionContext.toolCatalog = [
      {
        name: 'filesystem.read_file',
        description: 'Read files',
        category: 'Filesystem',
        input_schema: { type: 'object', required: ['path'] },
        output_schema: { type: 'object' },
        risk_level: 'Low',
        approval_policy: 'AutoApprove',
        timeout_ms: 10000,
        permissions: ['ReadWorkspace'],
      },
    ]
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/status',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(
      screen.getByRole('region', { name: 'Session status' }),
    ).toBeInTheDocument()
    expect(screen.getByText('status=Idle')).toBeInTheDocument()
    expect(screen.getByText('model=vertex/claude-sonnet-test')).toBeInTheDocument()
    expect(screen.getByText('workspace=/home/user/coddy')).toBeInTheDocument()
    expect(screen.getByText('tools=1')).toBeInTheDocument()
  })

  it('shows local slash command help from the /? slash command', async () => {
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/?',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(
      screen.getByRole('region', { name: 'Slash command help' }),
    ).toBeInTheDocument()
    expect(screen.getByText('/workspace')).toBeInTheDocument()
    expect(screen.getByText('/speak')).toBeInTheDocument()
  })

  it('routes workflow slash commands through the agent prompt instead of tab navigation', async () => {
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/review agent runtime',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).toHaveBeenCalledWith(
      expect.stringContaining('Code review workflow'),
    )
    expect(sessionContext.ask).toHaveBeenCalledWith(
      expect.stringContaining('Scope: agent runtime'),
    )
    expect(sessionContext.openUi).not.toHaveBeenCalled()
  })

  it('opens the workspace tab and triggers folder selection', async () => {
    sessionContext.activeWorkspacePath = '/home/user/project'
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/workspace',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))
    await userEvent.click(
      screen.getByRole('button', { name: 'Change workspace' }),
    )

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(screen.getByText('/home/user/project')).toBeInTheDocument()
    expect(sessionContext.selectWorkspaceFolder).toHaveBeenCalledOnce()
  })

  it('opens the workspace tab and runs the quality gate from /quality run', async () => {
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/quality run',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.ask).not.toHaveBeenCalled()
    expect(screen.getByText('Quality gate')).toBeInTheDocument()
    expect(sessionContext.runQualityEval).toHaveBeenCalledOnce()
  })

  it('loads history and starts new sessions from slash commands', async () => {
    sessionContext.conversationHistory = [
      {
        summary: {
          session_id: 'session-1',
          title: 'Analyze workspace',
          created_at_unix_ms: 1,
          updated_at_unix_ms: 2,
          message_count: 2,
          selected_model: { provider: 'openrouter', name: 'deepseek' },
          mode: 'DesktopApp',
        },
        messages: [],
      },
    ]
    sessionContext.conversationHistoryStatus = 'succeeded'
    render(<DesktopApp />)

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/history',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.loadConversationHistory).toHaveBeenCalledOnce()
    expect(screen.getByText('Analyze workspace')).toBeInTheDocument()

    await userEvent.click(
      screen.getByRole('button', { name: /Analyze workspace/i }),
    )

    expect(sessionContext.openConversation).toHaveBeenCalledWith('session-1')

    await userEvent.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      '/new',
    )
    await userEvent.click(screen.getByRole('button', { name: 'Send' }))

    expect(sessionContext.newSession).toHaveBeenCalledOnce()
  })
})
