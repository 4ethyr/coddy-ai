import { describe, expect, it, vi } from 'vitest'
import userEvent from '@testing-library/user-event'
import { render, screen } from '@testing-library/react'
import { WorkspacePanel } from '@/presentation/components/WorkspacePanel'

describe('WorkspacePanel', () => {
  it('renders backend tool catalog metadata with risk and approval state', () => {
    render(
      <WorkspacePanel
        items={[]}
        tools={[
          {
            name: 'filesystem.read_file',
            description: 'Read a UTF-8 text file inside the active workspace',
            category: 'Filesystem',
            input_schema: { type: 'object', required: ['path'] },
            output_schema: { type: 'object' },
            risk_level: 'Low',
            permissions: ['ReadWorkspace'],
            timeout_ms: 5_000,
            approval_policy: 'AutoApprove',
          },
          {
            name: 'shell.run',
            description: 'Execute a workspace-scoped shell command',
            category: 'Shell',
            input_schema: { type: 'object', required: ['command'] },
            output_schema: { type: 'object' },
            risk_level: 'Medium',
            permissions: ['ExecuteCommand'],
            timeout_ms: 30_000,
            approval_policy: 'AskOnUse',
          },
        ]}
      />,
    )

    expect(screen.getByText('filesystem.read_file')).toBeInTheDocument()
    expect(screen.getByText('shell.run')).toBeInTheDocument()
    expect(screen.getByText('input: path')).toBeInTheDocument()
    expect(screen.getByText('input: command')).toBeInTheDocument()
    expect(screen.getByText('Low')).toBeInTheDocument()
    expect(screen.getByText('AskOnUse')).toBeInTheDocument()
    expect(screen.getByText('ExecuteCommand')).toBeInTheDocument()
    expect(screen.getByText('auto-approved: 1')).toBeInTheDocument()
    expect(screen.getByText('approval required: 1')).toBeInTheDocument()
    expect(screen.getByText('highest risk: Medium')).toBeInTheDocument()
  })

  it('keeps an empty state when no tools are available yet', () => {
    render(<WorkspacePanel items={[]} tools={[]} />)

    expect(screen.getByText('No tools loaded yet')).toBeInTheDocument()
  })

  it('shows the selected workspace folder and triggers folder selection', async () => {
    const onSelectWorkspace = vi.fn()

    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        workspacePath="/home/user/project"
        onSelectWorkspace={onSelectWorkspace}
      />,
    )

    expect(screen.getByText('Workspace connected')).toBeInTheDocument()
    expect(screen.getByText('/home/user/project')).toBeInTheDocument()

    await userEvent.click(
      screen.getByRole('button', { name: 'Change workspace' }),
    )

    expect(onSelectWorkspace).toHaveBeenCalledOnce()
  })

  it('locks workspace selection while the dialog is open', () => {
    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        workspaceStatus="running"
        onSelectWorkspace={vi.fn()}
      />,
    )

    expect(
      screen.getByRole('button', { name: 'Select workspace folder' }),
    ).toBeDisabled()
    expect(screen.getByText('Selecting...')).toBeInTheDocument()
  })

  it('renders and triggers the multiagent eval harness summary', async () => {
    const onRun = vi.fn()

    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        multiagentEvalStatus="failed"
        onRunMultiagentEval={onRun}
        multiagentEval={{
          suite: {
            score: 92,
            passed: 3,
            failed: 1,
            reports: [
              {
                caseName: 'execution-reducer-contracts',
                status: 'passed',
                score: 100,
                failures: [],
                executionMetrics: {
                  total: 6,
                  completed: 6,
                  failed: 0,
                  blocked: 0,
                  awaitingApproval: 0,
                  acceptedOutputs: 6,
                  rejectedOutputs: 0,
                  missingOutputs: 0,
                  unexpectedOutputs: [],
                },
              },
            ],
          },
          baselineWritten: null,
          comparison: {
            status: 'failed',
            previousScore: 98,
            currentScore: 92,
            scoreDelta: -6,
            regressions: ['security-sensitive-routing'],
            improvements: [],
          },
        }}
      />,
    )

    expect(screen.getByText('Multiagent eval')).toBeInTheDocument()
    expect(screen.getByText('92')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByText('1')).toBeInTheDocument()
    expect(screen.getByText('-6')).toBeInTheDocument()
    expect(screen.getByText('baseline failed')).toBeInTheDocument()
    expect(screen.getByText('security-sensitive-routing')).toBeInTheDocument()
    expect(screen.getByText('Execution reducer')).toBeInTheDocument()
    expect(screen.getByText('6/6 completed')).toBeInTheDocument()
    expect(screen.getByText('accepted: 6')).toBeInTheDocument()
    expect(screen.getByText('rejected: 0')).toBeInTheDocument()
    expect(screen.getByText('missing: 0')).toBeInTheDocument()

    await userEvent.click(
      screen.getByRole('button', { name: 'Run multiagent eval' }),
    )

    expect(onRun).toHaveBeenCalledTimes(1)
    expect(onRun).toHaveBeenCalledWith({})
  })

  it('passes baseline paths from the workspace panel to the eval harness', async () => {
    const onRun = vi.fn()

    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        multiagentEvalStatus="idle"
        onRunMultiagentEval={onRun}
      />,
    )

    await userEvent.type(
      screen.getByLabelText('baseline'),
      ' /tmp/current-baseline.json ',
    )
    await userEvent.type(
      screen.getByLabelText('write baseline'),
      '/tmp/next-baseline.json',
    )
    await userEvent.click(
      screen.getByRole('button', { name: 'Run multiagent eval' }),
    )

    expect(onRun).toHaveBeenCalledWith({
      baseline: '/tmp/current-baseline.json',
      writeBaseline: '/tmp/next-baseline.json',
    })
  })

  it('preloads and emits eval harness baseline settings', async () => {
    const onSettingsChange = vi.fn()

    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        multiagentEvalStatus="idle"
        evalHarnessSettings={{
          baselinePath: '/tmp/coddy-baseline.json',
          writeBaselinePath: '',
        }}
        onEvalHarnessSettingsChange={onSettingsChange}
        onRunMultiagentEval={vi.fn()}
      />,
    )

    expect(screen.getByLabelText('baseline')).toHaveValue(
      '/tmp/coddy-baseline.json',
    )

    await userEvent.type(screen.getByLabelText('write baseline'), '/tmp/latest.json')

    expect(onSettingsChange).toHaveBeenLastCalledWith({
      baselinePath: '/tmp/coddy-baseline.json',
      writeBaselinePath: '/tmp/latest.json',
    })
  })

  it('locks the multiagent eval action while a run is active', () => {
    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        multiagentEvalStatus="running"
        onRunMultiagentEval={vi.fn()}
      />,
    )

    expect(
      screen.getByRole('button', { name: 'Run multiagent eval' }),
    ).toBeDisabled()
    expect(screen.getByText('Running')).toBeInTheDocument()
  })

  it('renders and triggers the prompt battery harness summary', async () => {
    const onRun = vi.fn()

    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        promptBatteryStatus="succeeded"
        onRunPromptBattery={onRun}
        promptBattery={{
          promptCount: 1200,
          stackCount: 30,
          knowledgeAreaCount: 10,
          passed: 1200,
          failed: 0,
          score: 100,
          memberCoverage: {
            explorer: 1200,
            reviewer: 1200,
            coder: 244,
          },
          failures: [],
        }}
      />,
    )

    expect(screen.getByText('Prompt battery')).toBeInTheDocument()
    expect(screen.getByText('prompts')).toBeInTheDocument()
    expect(screen.getByText('stacks')).toBeInTheDocument()
    expect(screen.getByText('explorer: 1200')).toBeInTheDocument()
    expect(screen.getByText('coder: 244')).toBeInTheDocument()

    await userEvent.click(
      screen.getByRole('button', { name: 'Run prompt battery' }),
    )

    expect(onRun).toHaveBeenCalledTimes(1)
  })

  it('renders and triggers the combined quality eval gate', async () => {
    const onRun = vi.fn()

    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        qualityEvalStatus="succeeded"
        onRunQualityEval={onRun}
        qualityEval={{
          kind: 'coddy.qualityEval',
          version: 1,
          status: 'passed',
          passed: true,
          score: 100,
          checks: [
            {
              name: 'multiagent',
              status: 'passed',
              score: 100,
              passed: 3,
              failed: 0,
            },
            {
              name: 'prompt-battery',
              status: 'passed',
              score: 100,
              promptCount: 1200,
              passed: 1200,
              failed: 0,
            },
          ],
          multiagent: {
            score: 100,
            passed: 3,
            failed: 0,
            reports: [],
          },
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
        }}
      />,
    )

    expect(screen.getByText('Quality gate')).toBeInTheDocument()
    expect(screen.getByText('status')).toBeInTheDocument()
    expect(screen.getByText('passed')).toBeInTheDocument()
    expect(screen.getByText('checks')).toBeInTheDocument()
    expect(screen.getByText('2')).toBeInTheDocument()
    expect(screen.getByText('multiagent: 100')).toBeInTheDocument()
    expect(screen.getByText('prompt-battery: 100')).toBeInTheDocument()

    await userEvent.click(
      screen.getByRole('button', { name: 'Run quality eval' }),
    )

    expect(onRun).toHaveBeenCalledTimes(1)
  })

  it('locks the prompt battery action while a run is active', () => {
    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        promptBatteryStatus="running"
        onRunPromptBattery={vi.fn()}
      />,
    )

    expect(
      screen.getByRole('button', { name: 'Run prompt battery' }),
    ).toBeDisabled()
    expect(screen.getByText('Running')).toBeInTheDocument()
  })

  it('locks the quality eval action while a run is active', () => {
    render(
      <WorkspacePanel
        items={[]}
        tools={[]}
        qualityEvalStatus="running"
        onRunQualityEval={vi.fn()}
      />,
    )

    expect(
      screen.getByRole('button', { name: 'Run quality eval' }),
    ).toBeDisabled()
    expect(screen.getByText('Running')).toBeInTheDocument()
  })
})
