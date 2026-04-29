import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ModelSelector } from '@/presentation/components/ModelSelector'

describe('ModelSelector', () => {
  it('renders the active model', () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    expect(screen.getByText('MODEL: gemma4-E2B')).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    ).toBeInTheDocument()
  })

  it('emits the selected model from the dropdown', async () => {
    const onSelect = vi.fn()
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onSelect={onSelect}
      />,
    )

    const trigger = screen.getByRole('button', {
      name: 'Active model ollama/gemma4-E2B',
    })
    expect(trigger).toHaveAttribute('aria-expanded', 'false')

    await userEvent.click(trigger)
    expect(trigger).toHaveAttribute('aria-expanded', 'true')

    await userEvent.click(
      screen.getByRole('button', { name: /qwen2.5:0.5b/ }),
    )

    expect(onSelect).toHaveBeenCalledWith({
      provider: 'ollama',
      name: 'qwen2.5:0.5b',
    })
  })

  it('renders first-class API provider groups', async () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    expect(screen.getByText('Local Ollama')).toBeInTheDocument()
    expect(screen.getByText('OpenAI')).toBeInTheDocument()
    expect(screen.getByText('OpenRouter')).toBeInTheDocument()
    expect(screen.getByText('Google Vertex')).toBeInTheDocument()
    expect(screen.getByText('Azure OpenAI')).toBeInTheDocument()
  })

  it('emits cloud provider selections without credential side effects', async () => {
    const onSelect = vi.fn()
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onSelect={onSelect}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Select GPT-4.1 via OpenAI' }),
    )

    expect(onSelect).toHaveBeenCalledWith({
      provider: 'openai',
      name: 'gpt-4.1',
    })
  })
})
