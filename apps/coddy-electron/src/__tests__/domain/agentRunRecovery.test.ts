import { describe, expect, it } from 'vitest'
import { buildAgentRunRecoveryNotice } from '@/domain'
import type { AgentRunSummary } from '@/domain'

function failedRun(
  overrides: Partial<AgentRunSummary> = {},
): AgentRunSummary {
  return {
    goal: 'analyze workspace',
    last_phase: 'Failed',
    completed_steps: 3,
    stop_reason: null,
    failure_code: 'invalid_provider_response',
    failure_message: 'Provider returned an empty response for sk-or-secret-token.',
    recoverable_failure: true,
    ...overrides,
  }
}

describe('agent run recovery notices', () => {
  it('turns recoverable OpenRouter failures into actionable guidance', () => {
    const notice = buildAgentRunRecoveryNotice(
      failedRun(),
      { provider: 'openrouter', name: 'deepseek/deepseek-v4-flash' },
    )

    expect(notice).toEqual({
      title: 'Recoverable model failure',
      technicalCode: 'invalid_provider_response',
      message: 'Provider returned an empty response for [REDACTED].',
      action:
        'Retry this prompt. If it repeats, reduce context/tool output, refresh OpenRouter routing, or select another provider/model.',
    })
  })

  it('redacts common provider tokens before rendering diagnostics', () => {
    const notice = buildAgentRunRecoveryNotice(
      failedRun({
        failure_message:
          'Bearer ya29.oauth-token and api-key: sk-live-secret failed.',
      }),
      { provider: 'openai', name: 'gpt-test' },
    )

    expect(notice?.message).toBe(
      'Bearer [REDACTED] and api-key: [REDACTED] failed.',
    )
  })

  it('does not produce a recovery notice when there is no failed run', () => {
    const notice = buildAgentRunRecoveryNotice(
      failedRun({
        last_phase: 'Completed',
        failure_code: null,
        failure_message: null,
        recoverable_failure: false,
      }),
      { provider: 'ollama', name: 'qwen2.5' },
    )

    expect(notice).toBeNull()
  })
})
