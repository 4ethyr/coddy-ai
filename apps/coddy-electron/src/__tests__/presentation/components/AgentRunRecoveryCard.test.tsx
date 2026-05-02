import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  buildAgentRunRecoveryNotice,
  type AgentRunSummary,
} from '@/domain'
import { AgentRunRecoveryCard } from '@/presentation/components/AgentRunRecoveryCard'

function failedRun(): AgentRunSummary {
  return {
    goal: 'analyze workspace',
    last_phase: 'Failed',
    completed_steps: 4,
    stop_reason: null,
    failure_code: 'invalid_provider_response',
    failure_message: 'Provider returned error for sk-or-secret-token.',
    recoverable_failure: true,
  }
}

describe('AgentRunRecoveryCard', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    })
  })

  it('copies redacted diagnostics for recoverable failures', async () => {
    const notice = buildAgentRunRecoveryNotice(
      failedRun(),
      { provider: 'openrouter', name: 'deepseek/deepseek-v4-flash' },
    )

    expect(notice).not.toBeNull()

    render(<AgentRunRecoveryCard notice={notice!} />)

    await userEvent.click(
      screen.getByRole('button', { name: /copy diagnostics/i }),
    )

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledTimes(1)
    })

    const diagnostics = vi.mocked(navigator.clipboard.writeText).mock
      .calls[0]?.[0]

    expect(diagnostics).toContain('technicalCode=invalid_provider_response')
    expect(diagnostics).toContain(
      'message=Provider returned error for [REDACTED].',
    )
    expect(diagnostics).toContain('refresh OpenRouter routing')
    expect(diagnostics).not.toContain('sk-or-secret-token')
    expect(screen.getByRole('button', { name: /copied/i })).toBeInTheDocument()
  })

  it('offers retry only when a retry action is available', async () => {
    const notice = buildAgentRunRecoveryNotice(
      failedRun(),
      { provider: 'openrouter', name: 'deepseek/deepseek-v4-flash' },
    )
    const onRetry = vi.fn()

    expect(notice).not.toBeNull()

    const { rerender } = render(<AgentRunRecoveryCard notice={notice!} />)

    expect(
      screen.queryByRole('button', { name: /retry prompt/i }),
    ).not.toBeInTheDocument()

    rerender(<AgentRunRecoveryCard notice={notice!} onRetry={onRetry} />)

    await userEvent.click(screen.getByRole('button', { name: /retry prompt/i }))

    expect(onRetry).toHaveBeenCalledTimes(1)
  })
})
