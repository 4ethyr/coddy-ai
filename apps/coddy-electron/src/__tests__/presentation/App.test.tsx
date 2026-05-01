import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import type { ReplSession } from '@/domain'
import { App } from '@/presentation/App'

const mocks = vi.hoisted(() => ({
  sessionContext: {
    session: {
      id: 'test-session',
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
    } as ReplSession,
    connecting: false,
    reconnecting: false,
    error: null,
    ask: vi.fn(),
    cancelRun: vi.fn(),
    cancelSpeech: vi.fn(),
    reconnect: vi.fn(),
    selectModel: vi.fn(),
    listProviderModels: vi.fn(),
    openUi: vi.fn(),
    captureVoice: vi.fn(),
    cancelVoiceCapture: vi.fn(),
    captureAndExplain: vi.fn(),
    dismissConfirmation: vi.fn(),
    replyPermission: vi.fn(),
  },
  modeContext: {
    mode: 'FloatingTerminal',
    setMode: vi.fn(),
  },
}))

vi.mock('@/presentation/hooks', () => ({
  SessionProvider: ({ children }: { children: ReactNode }) => children,
  ModeProvider: ({ children }: { children: ReactNode }) => children,
  useSessionContext: () => mocks.sessionContext,
  useMode: () => mocks.modeContext,
}))

describe('App keyboard cancellation', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.sessionContext.session = {
      ...mocks.sessionContext.session,
      mode: 'FloatingTerminal',
      status: 'Idle',
      active_run: null,
      voice: { enabled: true, speaking: false, muted: false },
    }
    mocks.modeContext.mode = 'FloatingTerminal'
    Object.defineProperty(window, 'replApi', {
      configurable: true,
      value: {
        invoke: vi.fn().mockResolvedValue({}),
        on: vi.fn(),
      },
    })
  })

  it('uses Escape to cancel an active run before closing the floating terminal', async () => {
    mocks.sessionContext.session = {
      ...mocks.sessionContext.session,
      status: 'Thinking',
      active_run: 'run-1',
    }

    render(<App />)

    fireEvent.keyDown(window, { key: 'Escape' })

    await waitFor(() => {
      expect(mocks.sessionContext.cancelRun).toHaveBeenCalledOnce()
    })
    expect(window.replApi.invoke).not.toHaveBeenCalledWith('window:close')
  })

  it('uses Escape to close the floating terminal only while idle', () => {
    render(<App />)

    fireEvent.keyDown(window, { key: 'Escape' })

    expect(mocks.sessionContext.cancelRun).not.toHaveBeenCalled()
    expect(window.replApi.invoke).toHaveBeenCalledWith('window:close')
  })
})
