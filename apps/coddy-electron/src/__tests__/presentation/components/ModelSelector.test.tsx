import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ModelSelector } from '@/presentation/components/ModelSelector'
import type { ModelProviderListRequest, ModelProviderListResult } from '@/domain'

function modelLoader(
  request: ModelProviderListRequest,
): Promise<ModelProviderListResult> {
  const models =
    request.provider === 'ollama'
      ? [
          {
            model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
            label: 'qwen2.5:0.5b',
            description: 'Local model from Ollama.',
            tags: ['local'],
          },
        ]
      : request.provider === 'openai'
        ? [
            {
              model: { provider: 'openai', name: 'gpt-4.1' },
              label: 'GPT-4.1',
              description: 'OpenAI API model.',
              tags: ['api', 'tools'],
            },
          ]
        : []

  return Promise.resolve({
    provider: request.provider,
    models,
    source: request.provider === 'ollama' ? 'local' : 'api',
    fetchedAtUnixMs: 1,
  })
}

function providerGroup(provider: string): HTMLElement {
  const group = screen
    .getByText(provider)
    .closest('[data-testid="model-provider-group"]')
  expect(group).toBeTruthy()
  return group as HTMLElement
}

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
        onLoadModels={modelLoader}
      />,
    )

    const trigger = screen.getByRole('button', {
      name: 'Active model ollama/gemma4-E2B',
    })
    expect(trigger).toHaveAttribute('aria-expanded', 'false')

    await userEvent.click(trigger)
    expect(trigger).toHaveAttribute('aria-expanded', 'true')

    await userEvent.click(
      await screen.findByRole('button', { name: /qwen2.5:0.5b/ }),
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

  it('loads provider models with a request-scoped API key', async () => {
    const onLoadModels = vi.fn(modelLoader)
    const onSelect = vi.fn()
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onLoadModels={onLoadModels}
        onSelect={onSelect}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const openAiGroup = providerGroup('OpenAI')
    await userEvent.type(within(openAiGroup).getByPlaceholderText('sk-...'), 'sk-test')
    await userEvent.click(within(openAiGroup).getByRole('button', { name: 'Load' }))

    await userEvent.click(
      await screen.findByRole('button', { name: 'Select GPT-4.1 via OpenAI' }),
    )

    expect(onLoadModels).toHaveBeenCalledWith({
      provider: 'openai',
      apiKey: 'sk-test',
      endpoint: undefined,
    })
    expect(onSelect).toHaveBeenCalledWith({
      provider: 'openai',
      name: 'gpt-4.1',
    })
  })

  it('filters providers and loaded models from dropdown search', async () => {
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onLoadModels={modelLoader}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    await screen.findByRole('button', { name: /qwen2.5:0.5b/ })
    await userEvent.type(
      screen.getByPlaceholderText('Search provider or model...'),
      'qwen',
    )

    expect(screen.getByText('Local Ollama')).toBeInTheDocument()
    expect(screen.queryByText('OpenAI')).not.toBeInTheDocument()
  })

  it('renders the dropdown as an isolated surface above terminal content', async () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    expect(screen.getByTestId('model-selector-menu')).toHaveClass(
      'model-selector-menu',
    )
  })

  it('keeps provider groups from shrinking so the menu can scroll', async () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const groups = screen.getAllByTestId('model-provider-group')
    expect(groups.length).toBeGreaterThan(1)
    for (const group of groups) {
      expect(group).toHaveClass('shrink-0')
    }
  })

  it('scrolls the provider catalog from wheel events inside the dropdown', async () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const menu = screen.getByTestId('model-selector-menu')
    Object.defineProperty(menu, 'clientHeight', {
      configurable: true,
      value: 120,
    })
    Object.defineProperty(menu, 'scrollHeight', {
      configurable: true,
      value: 520,
    })

    fireEvent.wheel(menu, { deltaY: 90 })
    expect(menu.scrollTop).toBe(90)

    fireEvent.wheel(menu, { deltaY: -200 })
    expect(menu.scrollTop).toBe(0)
  })

  it('emits cloud provider selections without credential side effects', async () => {
    const onSelect = vi.fn()
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onLoadModels={modelLoader}
        onSelect={onSelect}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const openAiGroup = providerGroup('OpenAI')
    await userEvent.type(within(openAiGroup).getByPlaceholderText('sk-...'), 'sk-test')
    await userEvent.click(within(openAiGroup).getByRole('button', { name: 'Load' }))

    await userEvent.click(await screen.findByRole('button', { name: 'Select GPT-4.1 via OpenAI' }))

    expect(onSelect).toHaveBeenCalledWith({
      provider: 'openai',
      name: 'gpt-4.1',
    })
  })
})
