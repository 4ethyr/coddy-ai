import { describe, expect, it, vi } from 'vitest'
import type { ReplIpcClient, ReplSessionSnapshot } from '@/domain'
import { applyEvents, createLocalSession, initializeSession } from '@/application'

function snapshot(): ReplSessionSnapshot {
  return {
    last_sequence: 42,
    session: {
      id: 'session-1',
      mode: 'DesktopApp',
      status: 'Idle',
      policy: 'UnknownAssessment',
      selected_model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
      voice: { enabled: true, speaking: false, muted: false },
      screen_context: null,
      workspace_context: [],
      messages: [],
      active_run: null,
      pending_permission: null,
    },
  }
}

describe('SessionManager', () => {
  it('initializes session state with backend tool catalog metadata', async () => {
    const client: ReplIpcClient = {
      getSnapshot: vi.fn().mockResolvedValue(snapshot()),
      getEventsAfter: vi.fn(),
      getToolCatalog: vi.fn().mockResolvedValue([
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
      ]),
      listProviderModels: vi.fn(),
      watchEvents: vi.fn(),
      ask: vi.fn(),
      voiceTurn: vi.fn(),
      stopActiveRun: vi.fn(),
      stopSpeaking: vi.fn(),
      selectModel: vi.fn(),
      openUi: vi.fn(),
      captureAndExplain: vi.fn(),
      dismissConfirmation: vi.fn(),
      replyPermission: vi.fn(),
      captureVoice: vi.fn(),
      cancelVoiceCapture: vi.fn(),
    }

    const state = await initializeSession(client)

    expect(client.getSnapshot).toHaveBeenCalledOnce()
    expect(client.getToolCatalog).toHaveBeenCalledOnce()
    expect(state.lastSequence).toBe(42)
    expect(state.session.subagent_activity).toEqual([])
    expect(state.toolCatalog).toEqual([
      expect.objectContaining({
        name: 'filesystem.read_file',
        risk_level: 'Low',
        approval_policy: 'AutoApprove',
      }),
    ])
  })

  it('keeps backend subagent lifecycle activity from snapshot', async () => {
    const nextSnapshot = snapshot()
    nextSnapshot.session.subagent_activity = [
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
    ]
    const client: ReplIpcClient = {
      getSnapshot: vi.fn().mockResolvedValue(nextSnapshot),
      getEventsAfter: vi.fn(),
      getToolCatalog: vi.fn().mockResolvedValue([]),
      listProviderModels: vi.fn(),
      watchEvents: vi.fn(),
      ask: vi.fn(),
      voiceTurn: vi.fn(),
      stopActiveRun: vi.fn(),
      stopSpeaking: vi.fn(),
      selectModel: vi.fn(),
      openUi: vi.fn(),
      captureAndExplain: vi.fn(),
      dismissConfirmation: vi.fn(),
      replyPermission: vi.fn(),
      captureVoice: vi.fn(),
      cancelVoiceCapture: vi.fn(),
    }

    const state = await initializeSession(client)

    expect(state.session.subagent_activity).toEqual([
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
    ])
  })

  it('starts local sessions with an empty tool catalog', () => {
    expect(createLocalSession().toolCatalog).toEqual([])
  })

  it('applies backend context item events into the UI session state', () => {
    const state = createLocalSession()

    const result = applyEvents(
      state,
      [
        {
          sequence: 1,
          session_id: 'session-1',
          run_id: 'run-1',
          captured_at_unix_ms: 1_775_000_000_000,
          event: {
            ContextItemAdded: {
              item: {
                id: 'tool:filesystem.read_file:.env',
                label: 'filesystem.read_file: .env',
                sensitive: true,
              },
            },
          },
        },
      ],
      1,
    )

    expect(result.lastSequence).toBe(1)
    expect(result.session.workspace_context).toEqual([
      {
        id: 'tool:filesystem.read_file:.env',
        label: 'filesystem.read_file: .env',
        sensitive: true,
      },
    ])
    expect(result.session.status).toBe('BuildingContext')
  })
})
