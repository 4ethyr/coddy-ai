import { describe, expect, it, vi } from 'vitest'
import type { ReplIpcClient } from '@/domain'
import {
  ReplCommandError,
  captureAndExplain,
  openUi,
  runMultiagentEval,
  runPromptBatteryEval,
  runQualityEval,
  selectModel,
  selectWorkspaceFolder,
  sendAsk,
  replyPermission,
  startNewSession,
  loadConversationHistory,
} from '@/application'

function clientWith(
  overrides: Partial<ReplIpcClient>,
): ReplIpcClient {
  return {
    getSnapshot: vi.fn(),
    getEventsAfter: vi.fn(),
    getToolCatalog: vi.fn(),
    getConversationHistory: vi.fn(),
    getActiveWorkspace: vi.fn(),
    selectWorkspaceFolder: vi.fn(),
    runMultiagentEval: vi.fn(),
    runPromptBatteryEval: vi.fn(),
    runQualityEval: vi.fn(),
    listProviderModels: vi.fn(),
    watchEvents: vi.fn(),
    ask: vi.fn(),
    voiceTurn: vi.fn(),
    stopActiveRun: vi.fn(),
    newSession: vi.fn(),
    openConversation: vi.fn(),
    stopSpeaking: vi.fn(),
    selectModel: vi.fn(),
    openUi: vi.fn(),
    captureAndExplain: vi.fn(),
    dismissConfirmation: vi.fn(),
    replyPermission: vi.fn(),
    captureVoice: vi.fn(),
    cancelVoiceCapture: vi.fn(),
    ...overrides,
  }
}

describe('CommandSender', () => {
  it('turns structured ask command errors into application errors', async () => {
    const client = clientWith({
      ask: vi.fn().mockResolvedValue({
        error: {
          code: 'provider_unavailable',
          message: 'OpenAI runtime adapter is not connected.',
        },
      }),
    })

    await expect(sendAsk(client, 'hello')).rejects.toMatchObject({
      code: 'provider_unavailable',
      message: 'OpenAI runtime adapter is not connected.',
    })
  })

  it('returns successful command payloads unchanged', async () => {
    const client = clientWith({
      openUi: vi.fn().mockResolvedValue({ text: 'Modo DesktopApp aberto.' }),
    })

    await expect(openUi(client, 'DesktopApp')).resolves.toEqual({
      text: 'Modo DesktopApp aberto.',
    })
  })

  it('propagates backend selection and screen-assist failures through one error type', async () => {
    const client = clientWith({
      selectModel: vi.fn().mockResolvedValue({
        error: { code: 'invalid_model', message: 'Model is not routable.' },
      }),
      captureAndExplain: vi.fn().mockResolvedValue({
        error: {
          code: 'assessment_policy_blocked',
          message: 'restricted assessments are blocked',
        },
      }),
    })

    await expect(
      selectModel(client, { provider: 'vertex', name: 'claude-test' }, 'Chat'),
    ).rejects.toBeInstanceOf(ReplCommandError)
    await expect(
      captureAndExplain(client, 'MultipleChoice', 'RestrictedAssessment'),
    ).rejects.toMatchObject({
      code: 'assessment_policy_blocked',
      message: 'restricted assessments are blocked',
    })
  })

  it('sends permission replies through the client port', async () => {
    const client = clientWith({
      replyPermission: vi.fn().mockResolvedValue({ message: 'approved' }),
    })

    await expect(replyPermission(client, 'perm-1', 'Once')).resolves.toEqual({
      message: 'approved',
    })
    expect(client.replyPermission).toHaveBeenCalledWith('perm-1', 'Once')
  })

  it('runs the multiagent eval harness through the client port', async () => {
    const result = {
      suite: { score: 100, passed: 2, failed: 0, reports: [] },
      baselineWritten: null,
    }
    const client = clientWith({
      runMultiagentEval: vi.fn().mockResolvedValue(result),
    })

    await expect(
      runMultiagentEval(client, { baseline: '/tmp/coddy-baseline.json' }),
    ).resolves.toEqual(result)
    expect(client.runMultiagentEval).toHaveBeenCalledWith({
      baseline: '/tmp/coddy-baseline.json',
    })
  })

  it('runs the prompt battery eval harness through the client port', async () => {
    const result = {
      promptCount: 1200,
      stackCount: 30,
      knowledgeAreaCount: 10,
      passed: 1200,
      failed: 0,
      score: 100,
      memberCoverage: { explorer: 1200 },
      failures: [],
    }
    const client = clientWith({
      runPromptBatteryEval: vi.fn().mockResolvedValue(result),
    })

    await expect(runPromptBatteryEval(client)).resolves.toEqual(result)
    expect(client.runPromptBatteryEval).toHaveBeenCalledWith()
  })

  it('runs the combined quality eval gate through the client port', async () => {
    const result = {
      kind: 'coddy.qualityEval',
      version: 1,
      status: 'passed' as const,
      passed: true,
      score: 100,
      checks: [
        { name: 'multiagent', status: 'passed' as const, score: 100, passed: 3, failed: 0 },
        {
          name: 'prompt-battery',
          status: 'passed' as const,
          score: 100,
          promptCount: 1200,
          passed: 1200,
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
    }
    const client = clientWith({
      runQualityEval: vi.fn().mockResolvedValue(result),
    })

    await expect(runQualityEval(client)).resolves.toEqual(result)
    expect(client.runQualityEval).toHaveBeenCalledWith()
  })

  it('selects a local workspace folder through the client port', async () => {
    const client = clientWith({
      selectWorkspaceFolder: vi.fn().mockResolvedValue({
        path: '/home/user/project',
        message: 'workspace set',
      }),
    })

    await expect(selectWorkspaceFolder(client)).resolves.toEqual({
      path: '/home/user/project',
      message: 'workspace set',
    })
    expect(client.selectWorkspaceFolder).toHaveBeenCalledOnce()
  })

  it('starts a new session and loads conversation history through the client port', async () => {
    const conversations = [
      {
        summary: {
          session_id: 'session-1',
          title: 'Review workspace',
          created_at_unix_ms: 1,
          updated_at_unix_ms: 2,
          message_count: 2,
          selected_model: { provider: 'openrouter', name: 'deepseek' },
          mode: 'DesktopApp' as const,
        },
        messages: [],
      },
    ]
    const client = clientWith({
      newSession: vi.fn().mockResolvedValue({ message: 'new session' }),
      getConversationHistory: vi.fn().mockResolvedValue(conversations),
    })

    await expect(startNewSession(client)).resolves.toEqual({
      message: 'new session',
    })
    await expect(loadConversationHistory(client, 10)).resolves.toEqual(
      conversations,
    )
    expect(client.newSession).toHaveBeenCalledOnce()
    expect(client.getConversationHistory).toHaveBeenCalledWith(10)
  })
})
