import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ConversationHistoryPanel } from '@/presentation/components/ConversationHistoryPanel'
import type { ConversationRecord } from '@/domain'

function record(id: string, title: string): ConversationRecord {
  return {
    summary: {
      session_id: id,
      title,
      created_at_unix_ms: 1_775_000_000_000,
      updated_at_unix_ms: 1_775_000_100_000,
      message_count: 2,
      selected_model: { provider: 'openrouter', name: 'deepseek/deepseek-v4-flash' },
      mode: 'FloatingTerminal',
    },
    messages: [
      { id: `${id}-user`, role: 'user', text: title },
      { id: `${id}-assistant`, role: 'assistant', text: 'Resposta redigida.' },
    ],
  }
}

describe('ConversationHistoryPanel', () => {
  it('renders history items as clickable controls and opens the selected session', async () => {
    const onSelect = vi.fn()

    render(
      <ConversationHistoryPanel
        records={[record('session-1', 'Review Coddy runtime')]}
        status="succeeded"
        error={null}
        onSelect={onSelect}
        onClose={() => {}}
      />,
    )

    const item = screen.getByRole('button', { name: /Review Coddy runtime/i })
    expect(item).toHaveClass('cursor-pointer')

    await userEvent.click(item)

    expect(onSelect).toHaveBeenCalledWith('session-1')
  })
})
