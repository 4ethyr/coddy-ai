import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { createInitialSession } from '@/domain'
import { CodingAgentCapabilitiesPanel } from '@/presentation/components/CodingAgentCapabilitiesPanel'

describe('CodingAgentCapabilitiesPanel', () => {
  it('summarizes the current coding-agent readiness without exposing secrets', () => {
    const session = createInitialSession('DesktopApp', {
      provider: 'openrouter',
      name: 'deepseek/deepseek-v4-flash',
    })
    session.agent_run = {
      run_id: 'run-1',
      summary: {
        goal: 'analyze codebase',
        last_phase: 'Completed',
        completed_steps: 4,
        stop_reason: null,
        failure_code: null,
        failure_message: null,
        recoverable_failure: false,
      },
    }
    session.subagent_activity = [
      {
        id: 'reviewer:review',
        name: 'reviewer',
        mode: 'read-only',
        status: 'Completed',
        readiness_score: 90,
        required_output_fields: ['approved', 'issues'],
        output_additional_properties_allowed: false,
        reason: 'code review completed',
      },
    ]

    render(
      <CodingAgentCapabilitiesPanel
        session={session}
        workspacePath="/home/user/project"
        toolCount={7}
        onClose={vi.fn()}
      />,
    )

    expect(
      screen.getByRole('region', { name: 'Coding agent capabilities' }),
    ).toBeInTheDocument()
    expect(screen.getByText('agent.capabilities')).toBeInTheDocument()
    expect(
      screen.getByText('model=openrouter/deepseek/deepseek-v4-flash'),
    ).toBeInTheDocument()
    expect(screen.getByText('7 registered tools are visible to the current session.')).toBeInTheDocument()
    expect(screen.getByText('/home/user/project')).toBeInTheDocument()
    expect(screen.getByText(/1 subagent activities/i)).toBeInTheDocument()
  })
})
