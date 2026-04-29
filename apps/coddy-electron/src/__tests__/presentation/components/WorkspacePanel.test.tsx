import { describe, expect, it } from 'vitest'
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
            risk_level: 'Low',
            permissions: ['ReadWorkspace'],
            timeout_ms: 5_000,
            approval_policy: 'AutoApprove',
          },
          {
            name: 'shell.run',
            description: 'Execute a workspace-scoped shell command',
            category: 'Shell',
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
    expect(screen.getByText('Low')).toBeInTheDocument()
    expect(screen.getByText('AskOnUse')).toBeInTheDocument()
    expect(screen.getByText('ExecuteCommand')).toBeInTheDocument()
  })

  it('keeps an empty state when no tools are available yet', () => {
    render(<WorkspacePanel items={[]} tools={[]} />)

    expect(screen.getByText('No tools loaded yet')).toBeInTheDocument()
  })
})
