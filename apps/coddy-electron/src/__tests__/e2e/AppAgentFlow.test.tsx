import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  ModelProviderListRequest,
  ModelProviderListResult,
  ModelRef,
  ModelRole,
  PermissionReply,
  ReplEvent,
  ReplEventEnvelope,
  ReplMode,
  ReplSessionSnapshot,
  ReplSessionSnapshotSession,
  ReplToolCatalogItem,
} from '@/domain'

interface SimDaemon {
  commands: string[]
  currentSequence: number
  events: ReplEventEnvelope[]
  listeners: Array<(data: unknown) => void>
  modelListRequests: ModelProviderListRequest[]
  snapshotSession: ReplSessionSnapshotSession
  toolCatalog: ReplToolCatalogItem[]
}

function createDaemon(): SimDaemon {
  return {
    commands: [],
    currentSequence: 0,
    events: [],
    listeners: [],
    modelListRequests: [],
    snapshotSession: {
      id: 'e2e-session',
      mode: 'DesktopApp',
      status: 'Idle',
      policy: 'Practice',
      selected_model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
      voice: { enabled: true, speaking: false, muted: false },
      screen_context: null,
      workspace_context: [],
      messages: [],
      active_run: null,
      pending_permission: null,
      tool_activity: [],
      subagent_activity: [],
      streaming_text: '',
    },
    toolCatalog: [
      {
        name: 'filesystem.read_file',
        description: 'Read a UTF-8 text file inside the active workspace',
        category: 'Filesystem',
        input_schema: { type: 'object', required: ['path'] },
        output_schema: { type: 'object' },
        risk_level: 'Low',
        permissions: ['ReadWorkspace'],
        timeout_ms: 5_000,
        approval_policy: 'AutoApprove',
      },
      {
        name: 'shell.run',
        description: 'Execute a workspace-scoped shell command',
        category: 'Shell',
        input_schema: { type: 'object', required: ['command'] },
        output_schema: { type: 'object' },
        risk_level: 'Medium',
        permissions: ['ExecuteCommand'],
        timeout_ms: 30_000,
        approval_policy: 'AskOnUse',
      },
      {
        name: 'subagent.reduce_outputs',
        description: 'Reduce strict subagent outputs into one safe summary.',
        category: 'Subagent',
        input_schema: { type: 'object', required: ['goal', 'outputs'] },
        output_schema: { type: 'object' },
        risk_level: 'Low',
        permissions: ['DelegateSubagent'],
        timeout_ms: 5_000,
        approval_policy: 'AutoApprove',
      },
    ],
  }
}

function installReplApi(daemon: SimDaemon): void {
  Object.defineProperty(window, 'replApi', {
    configurable: true,
    value: {
      invoke: vi.fn((channel: string, ...args: unknown[]) =>
        invokeChannel(daemon, channel, ...args),
      ),
      on: vi.fn((channel: string, callback: (data: unknown) => void) => {
        if (channel !== 'repl:watch-event') return () => {}
        daemon.listeners.push(callback)
        return () => {
          daemon.listeners = daemon.listeners.filter(
            (listener) => listener !== callback,
          )
        }
      }),
    },
  })
}

async function invokeChannel(
  daemon: SimDaemon,
  channel: string,
  ...args: unknown[]
): Promise<unknown> {
  switch (channel) {
    case 'repl:snapshot':
      return {
        session: clone(daemon.snapshotSession),
        last_sequence: daemon.currentSequence,
      } satisfies ReplSessionSnapshot

    case 'repl:tools':
      return daemon.toolCatalog

    case 'repl:watch-start':
      return { streamId: 'e2e-stream' }

    case 'repl:watch-close':
      return undefined

    case 'models:list': {
      const request = args[0] as ModelProviderListRequest
      daemon.modelListRequests.push(request)
      return {
        provider: request.provider,
        source: 'api',
        fetchedAtUnixMs: 1,
        models: [
          {
            model: { provider: 'openai', name: 'gpt-4.1' },
            label: 'GPT-4.1',
            description: 'OpenAI model exposed by the simulated backend.',
            tags: ['api', 'tools'],
          },
        ],
      } satisfies ModelProviderListResult
    }

    case 'repl:select-model': {
      const model = args[0] as ModelRef
      const role = args[1] as ModelRole
      daemon.commands.push(`select-model:${role}:${model.provider}/${model.name}`)
      pushEvent(daemon, { ModelSelected: { model, role } })
      return { text: `Modelo ${model.provider}/${model.name} selecionado.` }
    }

    case 'repl:ask': {
      const text = args[0] as string
      daemon.commands.push(`ask:${text}`)
      pushAskApprovalFlow(daemon, text)
      return { text: 'Tool approval required.' }
    }

    case 'repl:permission-reply': {
      const requestId = args[0] as string
      const reply = args[1] as PermissionReply
      daemon.commands.push(`permission-reply:${requestId}:${reply}`)
      pushApprovedToolCompletion(daemon, requestId, reply)
      return { text: `Permission ${reply} recorded.` }
    }

    case 'repl:open-ui': {
      const mode = args[0] as ReplMode
      daemon.commands.push(`open-ui:${mode}`)
      pushEvent(daemon, { OverlayShown: { mode } })
      return { text: `Mode ${mode} opened.` }
    }

    default:
      return {}
  }
}

function pushAskApprovalFlow(daemon: SimDaemon, text: string): void {
  const runId = 'run-e2e'
  pushEvent(daemon, {
    MessageAppended: {
      message: { id: 'msg-user', role: 'user', text },
    },
  })
  pushEvent(daemon, { RunStarted: { run_id: runId } })
  pushEvent(daemon, { ToolStarted: { name: 'shell.run' } })
  pushEvent(daemon, {
    PermissionRequested: {
      request: {
        id: 'perm-shell',
        session_id: 'e2e-session',
        run_id: runId,
        tool_call_id: 'tool-call-shell',
        tool_name: 'shell.run',
        permission: 'ExecuteCommand',
        patterns: ['cargo test -p coddy-agent'],
        risk_level: 'Medium',
        metadata: { command: 'cargo test -p coddy-agent' },
        requested_at_unix_ms: 1,
      },
    },
  })
}

function pushApprovedToolCompletion(
  daemon: SimDaemon,
  requestId: string,
  reply: PermissionReply,
): void {
  const runId = 'run-e2e'
  pushEvent(daemon, { PermissionReplied: { request_id: requestId, reply } })
  pushEvent(daemon, {
    ToolCompleted: { name: 'shell.run', status: 'Succeeded' },
  })
  pushEvent(daemon, {
    SubagentHandoffPrepared: {
      handoff: {
        name: 'reviewer',
        mode: 'read-only',
        approval_required: false,
        allowed_tools: ['filesystem.read_file', 'filesystem.search_files'],
        required_output_fields: ['approved', 'issues', 'suggestions'],
        output_additional_properties_allowed: false,
        timeout_ms: 60_000,
        max_context_tokens: 8_000,
        validation_checklist: ['review diff', 'report blocking risks'],
        safety_notes: ['do not expose secrets'],
        readiness_score: 100,
        readiness_issues: [],
      },
    },
  })
  pushEvent(daemon, {
    SubagentLifecycleUpdated: {
      update: {
        name: 'reviewer',
        mode: 'read-only',
        status: 'Approved',
        readiness_score: 100,
        reason: null,
      },
    },
  })
  pushEvent(daemon, {
    SubagentLifecycleUpdated: {
      update: {
        name: 'reviewer',
        mode: 'read-only',
        status: 'Running',
        readiness_score: 100,
        reason: null,
      },
    },
  })
  pushEvent(daemon, {
    SubagentLifecycleUpdated: {
      update: {
        name: 'reviewer',
        mode: 'read-only',
        status: 'Completed',
        readiness_score: 100,
        reason: null,
      },
    },
  })
  pushEvent(daemon, {
    TokenDelta: {
      run_id: runId,
      text: 'O fluxo validou mensagem, tool, aprovação e subagent.',
    },
  })
  pushEvent(daemon, {
    MessageAppended: {
      message: {
        id: 'msg-assistant',
        role: 'assistant',
        text: 'O fluxo validou mensagem, tool, aprovação e subagent.',
      },
    },
  })
  pushEvent(daemon, { RunCompleted: { run_id: runId } })
}

function pushEvent(daemon: SimDaemon, event: ReplEvent): void {
  applyEventToSnapshot(daemon, event)
  daemon.currentSequence += 1
  const envelope: ReplEventEnvelope = {
    sequence: daemon.currentSequence,
    session_id: 'e2e-session',
    run_id: 'run-e2e',
    captured_at_unix_ms: Date.now(),
    event,
  }
  daemon.events.push(envelope)
  for (const listener of daemon.listeners) {
    listener({ streamId: 'e2e-stream', done: false, event: envelope })
  }
}

function applyEventToSnapshot(daemon: SimDaemon, event: ReplEvent): void {
  const session = daemon.snapshotSession

  if ('MessageAppended' in event) {
    session.messages.push(event.MessageAppended.message)
    session.streaming_text = ''
  }

  if ('ModelSelected' in event && event.ModelSelected.role === 'Chat') {
    session.selected_model = event.ModelSelected.model
  }

  if ('OverlayShown' in event) {
    session.mode = event.OverlayShown.mode
  }

  if ('RunStarted' in event) {
    session.active_run = event.RunStarted.run_id
    session.status = 'Thinking'
    session.streaming_text = ''
    session.tool_activity = []
    session.subagent_activity = []
  }

  if ('ToolStarted' in event) {
    session.status = 'Thinking'
    session.tool_activity = [
      ...((session.tool_activity as unknown[]) ?? []),
      { id: 'shell.run-1', name: event.ToolStarted.name, status: 'Running' },
    ]
  }

  if ('PermissionRequested' in event) {
    session.status = 'AwaitingToolApproval'
    session.pending_permission = event.PermissionRequested.request
  }

  if ('PermissionReplied' in event) {
    session.pending_permission = null
    session.status = 'Thinking'
  }

  if ('ToolCompleted' in event) {
    session.tool_activity = [
      { id: 'shell.run-1', name: event.ToolCompleted.name, status: 'Succeeded' },
    ]
  }

  if ('SubagentHandoffPrepared' in event) {
    const handoff = event.SubagentHandoffPrepared.handoff
    session.subagent_activity = [
      {
        id: `${handoff.name}:${handoff.mode}`,
        name: handoff.name,
        mode: handoff.mode,
        status: 'Prepared',
        readiness_score: handoff.readiness_score,
        required_output_fields: handoff.required_output_fields,
        output_additional_properties_allowed:
          handoff.output_additional_properties_allowed,
        reason: null,
      },
    ]
  }

  if ('SubagentLifecycleUpdated' in event) {
    const update = event.SubagentLifecycleUpdated.update
    session.subagent_activity = [
      {
        id: `${update.name}:${update.mode}`,
        name: update.name,
        mode: update.mode,
        status: update.status,
        readiness_score: update.readiness_score,
        required_output_fields: ['approved', 'issues', 'suggestions'],
        output_additional_properties_allowed: false,
        reason: update.reason,
      },
    ]
  }

  if ('TokenDelta' in event) {
    session.status = 'Streaming'
    session.streaming_text = `${session.streaming_text ?? ''}${event.TokenDelta.text}`
  }

  if ('RunCompleted' in event) {
    session.active_run = null
    session.status = 'Idle'
    session.streaming_text = ''
  }
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

describe('App agent flow smoke', () => {
  let daemon: SimDaemon

  beforeEach(() => {
    vi.resetModules()
    window.localStorage.clear()
    daemon = createDaemon()
    installReplApi(daemon)
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('connects model selection, message streaming, tool approval and subagent activity', async () => {
    const user = userEvent.setup()
    const { App } = await import('@/presentation/App')

    render(<App />)

    expect(await screen.findByText('Coddy Core')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Neural_Link' }))
    await user.click(
      screen.getByRole('button', {
        name: 'Active model ollama/qwen2.5:0.5b',
      }),
    )

    const openAiGroup = screen
      .getByText('OpenAI')
      .closest('[data-testid="model-provider-group"]')
    expect(openAiGroup).toBeTruthy()
    await user.type(within(openAiGroup as HTMLElement).getByPlaceholderText('sk-...'), 'sk-e2e')
    await user.click(within(openAiGroup as HTMLElement).getByRole('button', { name: 'Load' }))
    await user.click(await screen.findByRole('button', { name: 'Select GPT-4.1 via OpenAI' }))

    expect(daemon.modelListRequests).toEqual(
      expect.arrayContaining([
        {
          provider: 'openai',
          apiKey: 'sk-e2e',
          apiVersion: undefined,
          endpoint: undefined,
        },
      ]),
    )
    expect(daemon.commands).toContain('select-model:Chat:openai/gpt-4.1')

    await user.click(screen.getByRole('button', { name: 'Terminal' }))
    await user.type(
      screen.getByPlaceholderText('Instruct Coddy agent...'),
      'rode os testes do agent com aprovação',
    )
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect(await screen.findByText('Tool approval')).toBeInTheDocument()
    expect(screen.getByText('shell.run')).toBeInTheDocument()
    expect(screen.getByText(/ExecuteCommand \/\/ Medium/)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Once' }))

    await waitFor(() => {
      expect(screen.queryByText('Tool approval')).not.toBeInTheDocument()
    })
    expect(
      screen.getByText('shell.run // Succeeded'),
    ).toBeInTheDocument()
    expect(
      screen.getByText(
        'subagent.reviewer // Completed // readiness=100 // output=approved, issues, suggestions // strict',
      ),
    ).toBeInTheDocument()
    expect(
      screen.getByText('O fluxo validou mensagem, tool, aprovação e subagent.'),
    ).toBeInTheDocument()
    expect(daemon.commands).toContain('permission-reply:perm-shell:Once')
  })
})
