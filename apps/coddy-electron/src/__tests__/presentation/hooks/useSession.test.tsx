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
    listProviderModels: vi.fn(),
    watchEvents: vi.fn().mockReturnValue(watchEventsNever()),
    ask: vi.fn(),
    voiceTurn: vi.fn(),
    stopActiveRun: vi.fn().mockResolvedValue(undefined),
    stopSpeaking: vi.fn().mockResolvedValue(undefined),
    selectModel: vi.fn(),
    openUi: vi.fn(),
    runMultiagentEval: vi.fn(),
    runPromptBatteryEval: vi.fn(),
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
})
