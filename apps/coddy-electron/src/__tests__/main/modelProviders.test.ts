import { describe, expect, it, vi } from 'vitest'
import { listProviderModels } from '../../main/modelProviders'

function jsonResponse(body: unknown) {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve(body),
  }
}

function errorResponse(status: number, statusText: string) {
  return {
    ok: false,
    status,
    statusText,
    json: () => Promise.resolve({}),
  }
}

describe('modelProviders', () => {
  it('lists OpenAI models with bearer authentication', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        data: [{ id: 'gpt-test', owned_by: 'openai' }],
      }),
    )

    const result = await listProviderModels(
      { provider: 'openai', apiKey: 'sk-test' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://api.openai.com/v1/models',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer sk-test',
        }),
      }),
    )
    expect(result.models).toEqual([
      expect.objectContaining({
        model: { provider: 'openai', name: 'gpt-test' },
      }),
    ])
  })

  it('lists Gemini API models with x-goog-api-key authentication', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        models: [
          {
            name: 'models/gemini-test',
            baseModelId: 'gemini-test',
            displayName: 'Gemini Test',
            supportedActions: ['generateContent'],
          },
        ],
      }),
    )

    const result = await listProviderModels(
      { provider: 'vertex', apiKey: 'AIza-test' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000',
      expect.objectContaining({
        headers: expect.objectContaining({
          'x-goog-api-key': 'AIza-test',
        }),
      }),
    )
    expect(result.models[0]).toMatchObject({
      model: { provider: 'vertex', name: 'gemini-test' },
      label: 'Gemini Test',
    })
    expect(result.notices).toEqual([
      'Gemini API keys list Gemini models only. Claude on Vertex requires a Google OAuth access token or Application Default Credentials.',
    ])
  })

  it('treats non-bearer Google credentials as Gemini API keys', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        models: [{ name: 'models/gemini-test' }],
      }),
    )

    const result = await listProviderModels(
      { provider: 'vertex', apiKey: 'google-key-without-aiza-prefix' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000',
      expect.objectContaining({
        headers: expect.objectContaining({
          'x-goog-api-key': 'google-key-without-aiza-prefix',
        }),
      }),
    )
    expect(result.notices?.[0]).toContain('Claude on Vertex requires')
  })

  it('uses Vertex AI OAuth only for explicit bearer credentials', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        publisherModels: [{ name: 'publishers/google/models/gemini-test' }],
      }),
    )

    await listProviderModels(
      { provider: 'vertex', apiKey: 'Bearer ya29.test-token' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://aiplatform.googleapis.com/v1beta1/publishers/google/models?pageSize=100&view=BASIC',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer ya29.test-token',
        }),
      }),
    )
  })

  it('lists Anthropic publisher models when using Vertex AI OAuth', async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [{ name: 'publishers/google/models/gemini-test' }],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [
            {
              name: 'publishers/anthropic/models/claude-sonnet-4-5@20250929',
              displayName: 'Claude Sonnet 4.5',
              launchStage: 'GA',
            },
            {
              name: 'publishers/anthropic/models/claude-opus-4-1@20250805',
              displayName: 'Claude Opus 4.1',
              launchStage: 'GA',
            },
          ],
        }),
      )

    const result = await listProviderModels(
      { provider: 'vertex', apiKey: 'Bearer ya29.test-token' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://us-east5-aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?pageSize=100&view=BASIC',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer ya29.test-token',
        }),
      }),
    )
    expect(result.models).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          model: {
            provider: 'vertex',
            name: 'claude-sonnet-4-5@20250929',
          },
          label: 'Claude Sonnet 4.5',
          tags: expect.arrayContaining(['vertex', 'anthropic', 'GA']),
        }),
        expect.objectContaining({
          model: {
            provider: 'vertex',
            name: 'claude-opus-4-1@20250805',
          },
          label: 'Claude Opus 4.1',
          tags: expect.arrayContaining(['vertex', 'anthropic', 'GA']),
        }),
      ]),
    )
  })

  it('uses an explicit Vertex region for publisher model listing', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        publisherModels: [{ name: 'publishers/anthropic/models/claude-test' }],
      }),
    )

    await listProviderModels(
      {
        provider: 'vertex',
        apiKey: 'Bearer ya29.test-token',
        endpoint: 'europe-west1',
      },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://europe-west1-aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?pageSize=100&view=BASIC',
      expect.anything(),
    )
  })

  it('rejects invalid Vertex region values before sending credentials', async () => {
    const fetcher = vi.fn()

    const result = await listProviderModels(
      {
        provider: 'vertex',
        apiKey: 'Bearer ya29.test-token',
        endpoint: '../secret',
      },
      fetcher,
    )

    expect(fetcher).not.toHaveBeenCalled()
    expect(result.error).toEqual(
      expect.objectContaining({
        code: 'MODEL_LIST_FAILED',
        message: 'Vertex region must be a region id like us-east5 or an HTTPS endpoint.',
      }),
    )
  })

  it('lists Vertex publisher models with Application Default Credentials', async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [{ name: 'publishers/google/models/gemini-test' }],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [
            {
              name: 'publishers/anthropic/models/claude-sonnet-4-5@20250929',
              displayName: 'Claude Sonnet 4.5',
              launchStage: 'GA',
            },
          ],
        }),
      )
    const tokenProvider = vi.fn().mockResolvedValue('adc-access-token')

    const result = await listProviderModels(
      { provider: 'vertex' },
      fetcher,
      tokenProvider,
    )

    expect(tokenProvider).toHaveBeenCalledWith(fetcher)
    expect(fetcher).toHaveBeenCalledWith(
      'https://us-east5-aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?pageSize=100&view=BASIC',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer adc-access-token',
        }),
      }),
    )
    expect(result.notices).toEqual([
      'Using Google Application Default Credentials for Vertex AI publisher models.',
    ])
    expect(result.models).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          model: {
            provider: 'vertex',
            name: 'claude-sonnet-4-5@20250929',
          },
        }),
      ]),
    )
  })

  it('reports a helpful Vertex credential error when no API key or ADC exists', async () => {
    const fetcher = vi.fn()
    const tokenProvider = vi.fn().mockResolvedValue(null)

    const result = await listProviderModels(
      { provider: 'vertex' },
      fetcher,
      tokenProvider,
    )

    expect(fetcher).not.toHaveBeenCalled()
    expect(result.error).toEqual(
      expect.objectContaining({
        code: 'MODEL_LIST_FAILED',
        message: expect.stringContaining('GOOGLE_APPLICATION_CREDENTIALS'),
      }),
    )
  })

  it('keeps available Vertex publisher models when one partner publisher fails', async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [{ name: 'publishers/google/models/gemini-test' }],
        }),
      )
      .mockResolvedValueOnce(errorResponse(403, 'Forbidden'))

    const result = await listProviderModels(
      { provider: 'vertex', apiKey: 'Bearer ya29.test-token' },
      fetcher,
    )

    expect(result.error).toBeUndefined()
    expect(result.models).toEqual([
      expect.objectContaining({
        model: { provider: 'vertex', name: 'gemini-test' },
        tags: expect.arrayContaining(['vertex', 'google']),
      }),
    ])
  })

  it('requires Azure model endpoints to use HTTPS before sending API keys', async () => {
    const fetcher = vi.fn()

    const result = await listProviderModels(
      {
        provider: 'azure',
        apiKey: 'azure-key',
        endpoint: 'http://resource.openai.azure.com',
      },
      fetcher,
    )

    expect(fetcher).not.toHaveBeenCalled()
    expect(result.error).toEqual(
      expect.objectContaining({
        code: 'MODEL_LIST_FAILED',
        message: 'Provider endpoint must use HTTPS.',
      }),
    )
  })
})
