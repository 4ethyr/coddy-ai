import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ReplIpcClient, ReplSessionSnapshot } from '@/domain'
import { useSession } from '@/presentation/hooks/useSession'

const clientRef = vi.hoisted(() => ({
  current: null as ReplIpcClient | null,
}))

vi.mock('@/presentation/hooks/useReplClient', () => ({
  useReplClient: () => {
    if (!clientRef.current) throw new Error('test client not configured')
    return clientRef.current
  },
}))

function snapshot(): ReplSessionSnapshot {
  return {
    last_sequence: 0,
    session: {
      id: 'session-1',
      mode: 'FloatingTerminal',
      status: 'Idle',
      policy: 'Practice',
      selected_model: { provider: 'ollama', name: 'qwen2.5' },
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
    },
  }
}

function watchEventsNever(): ReturnType<ReplIpcClient['watchEvents']> {
  return {
    [Symbol.asyncIterator]: () => ({
      next: () => new Promise(() => {}),
    }),
  }
}

function createClient(overrides: Partial<ReplIpcClient> = {}): ReplIpcClient {
  return {
    getSnapshot: vi.fn().mockResolvedValue(snapshot()),
    getEventsAfter: vi.fn(),
    getToolCatalog: vi.fn().mockResolvedValue([]),
    getConversationHistory: vi.fn().mockResolvedValue([]),
    getActiveWorkspace: vi.fn().mockResolvedValue({ path: null }),
    selectWorkspaceFolder: vi.fn().mockResolvedValue({ path: null, cancelled: true }),
    listProviderModels: vi.fn(),
    watchEvents: vi.fn().mockReturnValue(watchEventsNever()),
    ask: vi.fn(),
    voiceTurn: vi.fn(),
    stopActiveRun: vi.fn().mockResolvedValue(undefined),
    newSession: vi.fn().mockResolvedValue({ message: 'new session' }),
    openConversation: vi.fn().mockResolvedValue({ message: 'opened' }),
    stopSpeaking: vi.fn().mockResolvedValue(undefined),
    selectModel: vi.fn(),
    openUi: vi.fn(),
    runMultiagentEval: vi.fn(),
    runPromptBatteryEval: vi.fn(),
    runQualityEval: vi.fn(),
    captureAndExplain: vi.fn(),
    dismissConfirmation: vi.fn(),
    replyPermission: vi.fn(),
    captureVoice: vi.fn(),
    cancelVoiceCapture: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  }
}

describe('useSession cancellation errors', () => {
  beforeEach(() => {
    clientRef.current = null
  })

  it('surfaces active run cancellation failures as UI errors', async () => {
    clientRef.current = createClient({
      stopActiveRun: vi.fn().mockRejectedValue(new Error('daemon unavailable')),
    })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.cancelRun()
    })

    expect(result.current.error).toBe(
      'Coddy could not stop the active run: daemon unavailable',
    )

    unmount()
  })

  it('surfaces speech cancellation failures as UI errors', async () => {
    clientRef.current = createClient({
      stopSpeaking: vi.fn().mockRejectedValue(new Error('audio daemon busy')),
    })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.cancelSpeech()
    })

    expect(result.current.error).toBe(
      'Coddy could not stop speech playback: audio daemon busy',
    )

    unmount()
  })

  it('surfaces voice capture cancellation failures as UI errors', async () => {
    clientRef.current = createClient({
      cancelVoiceCapture: vi.fn().mockRejectedValue(new Error('capture is stuck')),
    })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.cancelVoiceCapture()
    })

    expect(result.current.error).toBe(
      'Coddy could not cancel voice capture: capture is stuck',
    )

    unmount()
  })

  it('loads redacted conversation history through the session hook', async () => {
    const conversations = [
      {
        summary: {
          session_id: 'session-1',
          title: 'Analyze workspace',
          created_at_unix_ms: 1,
          updated_at_unix_ms: 2,
          message_count: 2,
          selected_model: { provider: 'openrouter', name: 'deepseek' },
          mode: 'DesktopApp' as const,
        },
        messages: [],
      },
    ]
    clientRef.current = createClient({
      getConversationHistory: vi.fn().mockResolvedValue(conversations),
    })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.loadConversationHistory()
    })

    expect(result.current.conversationHistory).toEqual(conversations)
    expect(result.current.conversationHistoryStatus).toBe('succeeded')

    unmount()
  })

  it('opens a persisted conversation and refreshes the daemon snapshot', async () => {
    const openConversation = vi.fn().mockResolvedValue({ message: 'opened' })
    clientRef.current = createClient({ openConversation })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.openConversation('session-2')
    })

    expect(openConversation).toHaveBeenCalledWith('session-2')

    unmount()
  })

  it('passes voice response options to the capture backend', async () => {
    const captureVoice = vi.fn().mockResolvedValue({ text: 'voice command' })
    clientRef.current = createClient({ captureVoice })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.captureVoice({ speakResponse: true })
    })

    expect(captureVoice).toHaveBeenCalledWith({ speakResponse: true })

    unmount()
  })

  it('runs the combined quality eval and stores its status', async () => {
    const quality = {
      kind: 'coddy.qualityEval' as const,
      version: 1,
      status: 'passed' as const,
      passed: true,
      score: 100,
      checks: [
        {
          name: 'multiagent',
          status: 'passed' as const,
          score: 100,
          passed: 3,
          failed: 0,
        },
        {
          name: 'prompt-battery',
          status: 'passed' as const,
          score: 100,
          promptCount: 1200,
          passed: 1200,
          failed: 0,
        },
        {
          name: 'grounded-response',
          status: 'passed' as const,
          score: 100,
          caseCount: 3,
          passed: 3,
          failed: 0,
        },
      ],
      multiagent: { score: 100, passed: 3, failed: 0, reports: [] },
      promptBattery: {
        promptCount: 1200,
        stackCount: 30,
        knowledgeAreaCount: 10,
        passed: 1200,
        failed: 0,
        score: 100,
        memberCoverage: { explorer: 1200 },
        failures: [],
      },
      groundedResponse: {
        kind: 'coddy.groundedResponseEval' as const,
        caseCount: 3,
        passed: 3,
        failed: 0,
        score: 100,
        failures: [],
      },
    }
    clientRef.current = createClient({
      runQualityEval: vi.fn().mockResolvedValue(quality),
    })

    const { result, unmount } = renderHook(() => useSession())
    await waitFor(() => expect(result.current.connecting).toBe(false))

    await act(async () => {
      await result.current.runQualityEval()
    })

    expect(result.current.qualityEval).toEqual(quality)
    expect(result.current.qualityEvalStatus).toBe('succeeded')

    unmount()
  })
})
