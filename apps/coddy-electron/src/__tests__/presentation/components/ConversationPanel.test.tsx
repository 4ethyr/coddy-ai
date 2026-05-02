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
      provider: 'openrouter',
      name: 'deepseek/deepseek-v4-flash',
    })
    session.agent_run = {
      run_id: 'run-2',
      summary: {
        goal: 'debug timeout',
        last_phase: 'Failed',
        completed_steps: 2,
        stop_reason: null,
        failure_code: 'invalid_provider_response',
        failure_message: 'empty provider response with sk-or-secret-token',
        recoverable_failure: true,
      },
    }

    renderPanel(session)

    expect(screen.getByText('agent.run // Failed // steps=2')).toBeInTheDocument()
    expect(
      screen.getByText('failure // invalid_provider_response // recoverable'),
    ).toBeInTheDocument()
    expect(screen.getByText('Recoverable model failure')).toBeInTheDocument()
    expect(
      screen.getByText('empty provider response with [REDACTED]'),
    ).toBeInTheDocument()
    expect(
      screen.getByText(
        'Retry this prompt. If it repeats, reduce context/tool output, refresh OpenRouter routing, or select another provider/model.',
      ),
    ).toBeInTheDocument()
  })
})
