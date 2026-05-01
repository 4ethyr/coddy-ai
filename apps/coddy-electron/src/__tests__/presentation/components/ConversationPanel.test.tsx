import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { createInitialSession, type ReplSession } from '@/domain'
import { ConversationPanel } from '@/presentation/components/ConversationPanel'

function renderPanel(session: ReplSession) {
  return render(
    <ConversationPanel
      session={session}
      onSend={vi.fn()}
      onPermissionReply={vi.fn()}
    />,
  )
}

describe('ConversationPanel agent activity', () => {
  it('renders observable agent run phase and goal', () => {
    const session = createInitialSession('DesktopApp', {
      provider: 'openai',
      name: 'gpt-test',
    })
    session.active_run = 'run-1'
    session.agent_run = {
      run_id: 'run-1',
      summary: {
        goal: 'list files',
        last_phase: 'Inspecting',
        completed_steps: 2,
        stop_reason: null,
        failure_code: null,
        failure_message: null,
        recoverable_failure: false,
      },
    }

    renderPanel(session)

    expect(screen.getByText('agent.run // Inspecting // steps=2')).toBeInTheDocument()
    expect(screen.getByText('goal // list files')).toBeInTheDocument()
  })

  it('renders recoverable agent run failure details', () => {
    const session = createInitialSession('DesktopApp', {
      provider: 'openai',
      name: 'gpt-test',
    })
    session.agent_run = {
      run_id: 'run-2',
      summary: {
        goal: 'debug timeout',
        last_phase: 'Failed',
        completed_steps: 2,
        stop_reason: null,
        failure_code: 'transport_error',
        failure_message: 'provider timeout',
        recoverable_failure: true,
      },
    }

    renderPanel(session)

    expect(screen.getByText('agent.run // Failed // steps=2')).toBeInTheDocument()
    expect(screen.getByText('failure // transport_error // recoverable')).toBeInTheDocument()
  })
})
