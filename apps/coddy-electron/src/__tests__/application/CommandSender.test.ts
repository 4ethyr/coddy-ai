import { describe, expect, it, vi } from 'vitest'
import type { ReplIpcClient } from '@/domain'
import {
  ReplCommandError,
  captureAndExplain,
  openUi,
  selectModel,
  sendAsk,
} from '@/application'

function clientWith(
  overrides: Partial<ReplIpcClient>,
): ReplIpcClient {
  return {
    getSnapshot: vi.fn(),
    getEventsAfter: vi.fn(),
    getToolCatalog: vi.fn(),
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
})
