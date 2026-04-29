import { describe, expect, it, vi } from 'vitest'
import type { ReplIpcClient, ReplSessionSnapshot } from '@/domain'
import { createLocalSession, initializeSession } from '@/application'

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
          risk_level: 'Low',
          permissions: ['ReadWorkspace'],
          timeout_ms: 5_000,
          approval_policy: 'AutoApprove',
        },
      ]),
      watchEvents: vi.fn(),
      ask: vi.fn(),
      voiceTurn: vi.fn(),
      stopActiveRun: vi.fn(),
      stopSpeaking: vi.fn(),
      selectModel: vi.fn(),
      openUi: vi.fn(),
      captureAndExplain: vi.fn(),
      dismissConfirmation: vi.fn(),
      captureVoice: vi.fn(),
      cancelVoiceCapture: vi.fn(),
    }

    const state = await initializeSession(client)

    expect(client.getSnapshot).toHaveBeenCalledOnce()
    expect(client.getToolCatalog).toHaveBeenCalledOnce()
    expect(state.lastSequence).toBe(42)
    expect(state.toolCatalog).toEqual([
      expect.objectContaining({
        name: 'filesystem.read_file',
        risk_level: 'Low',
        approval_policy: 'AutoApprove',
      }),
    ])
  })

  it('starts local sessions with an empty tool catalog', () => {
    expect(createLocalSession().toolCatalog).toEqual([])
  })
})
