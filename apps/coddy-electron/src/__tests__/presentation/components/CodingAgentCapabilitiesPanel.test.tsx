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
        tools={[
          {
            name: 'filesystem.read_file',
            description: 'Read files',
            category: 'Filesystem',
            input_schema: { type: 'object' },
            output_schema: { type: 'object' },
            risk_level: 'Low',
            permissions: ['ReadWorkspace'],
            timeout_ms: 10000,
            approval_policy: 'AutoApprove',
          },
          {
            name: 'filesystem.apply_edit',
            description: 'Apply edits',
            category: 'Filesystem',
            input_schema: { type: 'object' },
            output_schema: { type: 'object' },
            risk_level: 'Medium',
            permissions: ['WriteWorkspace'],
            timeout_ms: 10000,
            approval_policy: 'AlwaysAsk',
          },
          {
            name: 'shell.run',
            description: 'Run shell commands',
            category: 'Shell',
            input_schema: { type: 'object' },
            output_schema: { type: 'object' },
            risk_level: 'High',
            permissions: ['ExecuteCommand'],
            timeout_ms: 30000,
            approval_policy: 'AskOnUse',
          },
        ]}
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
    expect(screen.getByText('3 registered tools are visible to the current session.')).toBeInTheDocument()
    expect(screen.getByText('1 auto-approved, 2 require approval, 0 denied.')).toBeInTheDocument()
    expect(screen.getByText('1 low, 1 medium, 1 high/critical.')).toBeInTheDocument()
    expect(screen.getByText('/home/user/project')).toBeInTheDocument()
    expect(screen.getByText(/1 subagent activities/i)).toBeInTheDocument()
  })
})
