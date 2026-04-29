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
  })

  it('treats non-bearer Google credentials as Gemini API keys', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        models: [{ name: 'models/gemini-test' }],
      }),
    )

    await listProviderModels(
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
