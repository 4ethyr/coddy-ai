import { afterEach, describe, expect, it, vi } from 'vitest'
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
  afterEach(() => {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      value: 1024,
    })
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      value: 768,
    })
  })

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

  it('labels which providers are executable by the current Rust runtime', async () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    expect(within(providerGroup('Local Ollama')).getByText('runtime ready')).toBeInTheDocument()
    expect(within(providerGroup('OpenAI')).getByText('runtime ready')).toBeInTheDocument()
    expect(within(providerGroup('OpenRouter')).getByText('runtime ready')).toBeInTheDocument()
    expect(within(providerGroup('Google Vertex')).getByText('runtime ready')).toBeInTheDocument()
    expect(within(providerGroup('Azure OpenAI')).getByText('runtime ready')).toBeInTheDocument()
  })

  it('allows Vertex model loading through local gcloud or ADC without pasting a token', async () => {
    const onLoadModels = vi.fn(modelLoader)
    render(
      <ModelSelector
        model={{ provider: 'vertex', name: 'gemini-2.5-flash' }}
        onLoadModels={onLoadModels}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', {
        name: 'Active model vertex/gemini-2.5-flash',
      }),
    )

    const vertexGroup = providerGroup('Google Vertex')
    expect(within(vertexGroup).getByText('local auth or token')).toBeInTheDocument()
    expect(
      within(vertexGroup).getByText('Load with local gcloud/ADC or paste a token.'),
    ).toBeInTheDocument()

    await userEvent.click(within(vertexGroup).getByRole('button', { name: 'Load' }))

    expect(onLoadModels).toHaveBeenCalledWith({
      provider: 'vertex',
      apiKey: undefined,
      endpoint: undefined,
    })
  })

  it('allows Vertex model loading with a region endpoint value', async () => {
    const onLoadModels = vi.fn(modelLoader)
    render(
      <ModelSelector
        model={{ provider: 'vertex', name: 'gemini-2.5-flash' }}
        onLoadModels={onLoadModels}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', {
        name: 'Active model vertex/gemini-2.5-flash',
      }),
    )

    const vertexGroup = providerGroup('Google Vertex')
    await userEvent.type(
      within(vertexGroup).getByPlaceholderText(
        'global, us-east5 ou https://...',
      ),
      'us-east5',
    )
    await userEvent.click(within(vertexGroup).getByRole('button', { name: 'Load' }))

    expect(onLoadModels).toHaveBeenCalledWith({
      provider: 'vertex',
      apiKey: undefined,
      endpoint: 'us-east5',
    })
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

  it('loads Azure deployments with endpoint and API version', async () => {
    const onLoadModels = vi.fn(modelLoader)
    render(
      <ModelSelector
        model={{ provider: 'azure', name: 'gpt-4.1-coddy' }}
        onLoadModels={onLoadModels}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', {
        name: 'Active model azure/gpt-4.1-coddy',
      }),
    )

    const azureGroup = providerGroup('Azure OpenAI')
    await userEvent.type(
      within(azureGroup).getByPlaceholderText('api-key'),
      'azure-key',
    )
    await userEvent.type(
      within(azureGroup).getByPlaceholderText('https://resource.openai.azure.com'),
      'https://coddy-resource.openai.azure.com',
    )
    await userEvent.type(
      within(azureGroup).getByPlaceholderText('2024-10-21'),
      '2025-01-01-preview',
    )
    await userEvent.click(within(azureGroup).getByRole('button', { name: 'Load' }))

    expect(onLoadModels).toHaveBeenCalledWith({
      provider: 'azure',
      apiKey: 'azure-key',
      endpoint: 'https://coddy-resource.openai.azure.com',
      apiVersion: '2025-01-01-preview',
    })
  })

  it('can request secure credential persistence without keeping the token in the form', async () => {
    const onLoadModels = vi.fn(async (request: ModelProviderListRequest) => {
      const result = await modelLoader(request)
      if (request.provider !== 'openai') return result

      return {
        ...result,
        credentialStorage: {
          persisted: true,
          message: 'Credential saved with secure OS encryption.',
        },
      }
    })
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onLoadModels={onLoadModels}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const openAiGroup = providerGroup('OpenAI')
    const apiKeyInput = within(openAiGroup).getByPlaceholderText('sk-...')
    await userEvent.type(apiKeyInput, 'sk-test')
    await userEvent.click(within(openAiGroup).getByLabelText(/Remember securely/))
    await userEvent.click(within(openAiGroup).getByRole('button', { name: 'Load' }))

    expect(onLoadModels).toHaveBeenCalledWith({
      provider: 'openai',
      apiKey: 'sk-test',
      endpoint: undefined,
      rememberCredential: true,
    })
    expect(apiKeyInput).toHaveValue('')
    expect(await screen.findByText(/saved securely/)).toBeInTheDocument()
  })

  it('clears API key drafts even when provider loading fails', async () => {
    const onLoadModels = vi.fn((request: ModelProviderListRequest) => {
      if (request.provider === 'ollama') return modelLoader(request)

      return Promise.resolve({
        provider: request.provider,
        models: [],
        source: 'api' as const,
        fetchedAtUnixMs: 1,
        error: { code: 'MODEL_LIST_FAILED', message: 'Provider unavailable.' },
      })
    })
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onLoadModels={onLoadModels}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const openAiGroup = providerGroup('OpenAI')
    const apiKeyInput = within(openAiGroup).getByPlaceholderText('sk-...')
    await userEvent.type(apiKeyInput, 'sk-test')
    await userEvent.click(within(openAiGroup).getByRole('button', { name: 'Load' }))

    expect(
      await within(openAiGroup).findByText('Provider unavailable.'),
    ).toBeInTheDocument()
    expect(apiKeyInput).toHaveValue('')
  })

  it('shows provider notices after loading models', async () => {
    const onLoadModels = vi.fn((request: ModelProviderListRequest) => {
      if (request.provider === 'ollama') return modelLoader(request)

      return Promise.resolve({
        provider: request.provider,
        models: [
          {
            model: { provider: 'vertex', name: 'gemini-test' },
            label: 'Gemini Test',
            description: 'Google Gemini API model.',
            tags: ['api'],
          },
        ],
        source: 'api' as const,
        fetchedAtUnixMs: 1,
        notices: [
          'Gemini API keys list Gemini models only. Claude on Vertex requires a Google OAuth access token or Application Default Credentials.',
        ],
      })
    })

    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onLoadModels={onLoadModels}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    )

    const vertexGroup = providerGroup('Google Vertex')
    await userEvent.type(
      within(vertexGroup).getByPlaceholderText('API key, Bearer token, or leave blank for gcloud'),
      'google-api-key',
    )
    await userEvent.click(within(vertexGroup).getByRole('button', { name: 'Load' }))

    expect(
      await within(vertexGroup).findByText(/Claude on Vertex requires/),
    ).toBeInTheDocument()
  })

  it('keeps the dropdown frame inside narrow floating windows', async () => {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      value: 480,
    })
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      value: 420,
    })

    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    const trigger = screen.getByRole('button', {
      name: 'Active model ollama/gemma4-E2B',
    })
    trigger.getBoundingClientRect = () =>
      ({
        bottom: 48,
        height: 28,
        left: 300,
        right: 468,
        top: 20,
        width: 168,
        x: 300,
        y: 20,
        toJSON: () => ({}),
      }) as DOMRect

    await userEvent.click(trigger)

    const popover = screen.getByTestId('model-selector-popover')
    expect(popover).toHaveStyle({ left: '12px', width: '456px' })
    expect(screen.getByTestId('model-selector-menu')).toHaveStyle({
      maxHeight: '352px',
    })
  })

  it('keeps the dropdown compact when the floating terminal is not fullscreen', async () => {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      value: 1383,
    })
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      value: 969,
    })

    render(
      <ModelSelector model={{ provider: 'vertex', name: 'gemini-2.5-flash' }} />,
    )

    const trigger = screen.getByRole('button', {
      name: 'Active model vertex/gemini-2.5-flash',
    })
    trigger.getBoundingClientRect = () =>
      ({
        bottom: 58,
        height: 32,
        left: 442,
        right: 895,
        top: 26,
        width: 453,
        x: 442,
        y: 26,
        toJSON: () => ({}),
      }) as DOMRect

    await userEvent.click(trigger)

    const popover = screen.getByTestId('model-selector-popover')
    expect(popover).toHaveStyle({ left: '335px', width: '560px' })
    expect(screen.getByTestId('model-selector-menu')).toHaveStyle({
      maxHeight: '620px',
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
