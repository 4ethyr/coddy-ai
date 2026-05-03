import { describe, expect, it, vi } from 'vitest'
import { mkdtempSync, rmSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import {
  listProviderModels,
  resolveGoogleApplicationDefaultAccessToken,
  resolveGcloudAccessToken,
  resolveGcloudProjectId,
} from '../../main/modelProviders'

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
  const vertexPublisherQuery =
    'pageSize=200&listAllVersions=true'

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

  it('lists OpenRouter text models with bearer authentication', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        data: [
          {
            id: 'anthropic/claude-sonnet-4.5',
            name: 'Claude Sonnet 4.5',
            description: 'Anthropic model routed through OpenRouter.',
            context_length: 200000,
          },
        ],
      }),
    )

    const result = await listProviderModels(
      { provider: 'openrouter', apiKey: 'sk-or-test' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://openrouter.ai/api/v1/models?output_modalities=text',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer sk-or-test',
        }),
      }),
    )
    expect(result.models).toEqual([
      expect.objectContaining({
        model: {
          provider: 'openrouter',
          name: 'anthropic/claude-sonnet-4.5',
        },
        label: 'Claude Sonnet 4.5',
        tags: ['api', '200000 ctx'],
      }),
    ])
  })

  it('reports a helpful OpenRouter credential error before network calls', async () => {
    const fetcher = vi.fn()

    const result = await listProviderModels(
      { provider: 'openrouter' },
      fetcher,
    )

    expect(fetcher).not.toHaveBeenCalled()
    expect(result.error).toEqual({
      code: 'MODEL_LIST_FAILED',
      message: 'OpenRouter API key is required.',
    })
  })

  it('lists NVIDIA NIM models with bearer authentication', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        data: [
          {
            id: 'deepseek-ai/deepseek-v4-pro',
            owned_by: 'deepseek-ai',
          },
          {
            id: 'meta/llama-test',
            owned_by: 'meta',
          },
        ],
      }),
    )

    const result = await listProviderModels(
      { provider: 'nvidia', apiKey: 'nvapi-test' },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      'https://integrate.api.nvidia.com/v1/models',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer nvapi-test',
        }),
      }),
    )
    expect(result.models).toEqual([
      expect.objectContaining({
        model: { provider: 'nvidia', name: 'deepseek-ai/deepseek-v4-pro' },
        label: 'deepseek-ai/deepseek-v4-pro',
        tags: ['api', 'deepseek-ai'],
      }),
      expect.objectContaining({
        model: { provider: 'nvidia', name: 'meta/llama-test' },
      }),
    ])
  })

  it('keeps DeepSeek V4 Pro visible when NVIDIA model listing omits it', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        data: [{ id: 'meta/llama-test', owned_by: 'meta' }],
      }),
    )

    const result = await listProviderModels(
      { provider: 'nvidia', apiKey: 'nvapi-test' },
      fetcher,
    )

    expect(result.models).toContainEqual(
      expect.objectContaining({
        model: { provider: 'nvidia', name: 'deepseek-ai/deepseek-v4-pro' },
        label: 'DeepSeek V4 Pro',
      }),
    )
  })

  it('reports a helpful NVIDIA credential error before network calls', async () => {
    const fetcher = vi.fn()

    const result = await listProviderModels(
      { provider: 'nvidia' },
      fetcher,
    )

    expect(fetcher).not.toHaveBeenCalled()
    expect(result.error).toEqual({
      code: 'MODEL_LIST_FAILED',
      message: 'NVIDIA API key is required.',
    })
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

  it('filters Gemini API models that are not compatible with text chat generation', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        models: [
          {
            name: 'models/gemini-3.1-flash-live-preview',
            baseModelId: 'gemini-3.1-flash-live-preview',
            displayName: 'Gemini Live Preview',
            supportedActions: ['bidiGenerateContent'],
          },
          {
            name: 'models/gemini-2.5-flash',
            baseModelId: 'gemini-2.5-flash',
            displayName: 'Gemini 2.5 Flash',
            supportedActions: ['generateContent'],
          },
        ],
      }),
    )

    const result = await listProviderModels(
      { provider: 'vertex', apiKey: 'AIza-test' },
      fetcher,
    )

    expect(result.models.map((entry) => entry.model.name)).toEqual([
      'gemini-2.5-flash',
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
      `https://aiplatform.googleapis.com/v1beta1/publishers/google/models?${vertexPublisherQuery}`,
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
      .mockResolvedValue(jsonResponse({ publisherModels: [] }))
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
      `https://aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?${vertexPublisherQuery}`,
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
      `https://europe-west1-aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?${vertexPublisherQuery}`,
      expect.anything(),
    )
  })

  it('requests all Vertex publisher versions across paginated model garden results', async () => {
    const fetcher = vi.fn((url: string) => {
      if (url.includes('/publishers/anthropic/models') && url.includes('pageToken=next')) {
        return Promise.resolve(
          jsonResponse({
            publisherModels: [
              {
                name: 'publishers/anthropic/models/claude-opus-4-5@20251101',
                displayName: 'Claude Opus 4.5',
                launchStage: 'GA',
              },
            ],
          }),
        )
      }

      if (url.includes('/publishers/anthropic/models')) {
        return Promise.resolve(
          jsonResponse({
            publisherModels: [
              {
                name: 'publishers/anthropic/models/claude-sonnet-4-5@20250929',
                displayName: 'Claude Sonnet 4.5',
                launchStage: 'GA',
              },
            ],
            nextPageToken: 'next',
          }),
        )
      }

      return Promise.resolve(jsonResponse({ publisherModels: [] }))
    })

    const result = await listProviderModels(
      {
        provider: 'vertex',
        apiKey: 'Bearer ya29.test-token',
        endpoint: 'us-east5',
      },
      fetcher,
    )

    expect(fetcher).toHaveBeenCalledWith(
      `https://us-east5-aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?${vertexPublisherQuery}&pageToken=next`,
      expect.anything(),
    )
    expect(result.models).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          model: {
            provider: 'vertex',
            name: 'claude-opus-4-5@20251101',
          },
          label: 'Claude Opus 4.5',
        }),
      ]),
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
      .mockResolvedValue(jsonResponse({ publisherModels: [] }))
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
    const tokenProvider = vi.fn().mockResolvedValue({
      token: 'adc-access-token',
      notice: 'Using Google Application Default Credentials for Vertex AI publisher models.',
      quotaProjectId: 'coddy-dev',
    })

    const result = await listProviderModels(
      { provider: 'vertex' },
      fetcher,
      tokenProvider,
    )

    expect(tokenProvider).toHaveBeenCalledWith(fetcher)
    expect(fetcher).toHaveBeenCalledWith(
      `https://aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?${vertexPublisherQuery}`,
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer adc-access-token',
          'x-goog-user-project': 'coddy-dev',
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

  it('lists Vertex publisher models with local gcloud OAuth fallback', async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(jsonResponse({ publisherModels: [] }))
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [{ name: 'publishers/google/models/gemini-test' }],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          publisherModels: [
            {
              name: 'publishers/anthropic/models/claude-sonnet-test',
              displayName: 'Claude Sonnet Test',
            },
          ],
        }),
      )
    const tokenProvider = vi.fn().mockResolvedValue({
      token: 'gcloud-access-token',
      notice:
        'Using local gcloud OAuth credentials for Vertex AI publisher models. The access token is short-lived and is not stored by Coddy.',
    })

    const result = await listProviderModels(
      { provider: 'vertex' },
      fetcher,
      tokenProvider,
    )

    expect(fetcher).toHaveBeenCalledWith(
      `https://aiplatform.googleapis.com/v1beta1/publishers/anthropic/models?${vertexPublisherQuery}`,
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer gcloud-access-token',
        }),
      }),
    )
    expect(result.notices?.[0]).toContain('gcloud OAuth credentials')
    expect(result.models).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          model: {
            provider: 'vertex',
            name: 'claude-sonnet-test',
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
        message: expect.stringContaining('gcloud auth'),
      }),
    )
  })

  it('resolves local gcloud access tokens without exposing command output', async () => {
    const runner = vi.fn((_file, _args, _options, callback) => {
      callback(null, 'ya29.gcloud-token\n', '')
    })

    await expect(resolveGcloudAccessToken(runner)).resolves.toBe(
      'ya29.gcloud-token',
    )
    expect(runner).toHaveBeenCalledWith(
      'gcloud',
      ['auth', 'print-access-token'],
      expect.objectContaining({
        maxBuffer: 4096,
        timeout: 10_000,
        windowsHide: true,
      }),
      expect.any(Function),
    )
  })

  it('prefers local gcloud credentials over ADC for host-login model discovery', async () => {
    const previousCredentialPath = process.env.GOOGLE_APPLICATION_CREDENTIALS
    const directory = mkdtempSync(join(tmpdir(), 'coddy-adc-'))
    const credentialPath = join(directory, 'application_default_credentials.json')
    writeFileSync(
      credentialPath,
      JSON.stringify({
        type: 'authorized_user',
        client_id: 'client-id',
        client_secret: 'client-secret',
        refresh_token: 'refresh-token',
        token_uri: 'https://oauth2.googleapis.com/token',
      }),
    )
    process.env.GOOGLE_APPLICATION_CREDENTIALS = credentialPath

    const fetcher = vi.fn()
    const gcloudTokenProvider = vi.fn().mockResolvedValue('ya29.gcloud-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    try {
      await expect(
        resolveGoogleApplicationDefaultAccessToken(
          fetcher,
          gcloudTokenProvider,
          gcloudProjectProvider,
        ),
      ).resolves.toEqual({
        token: 'ya29.gcloud-token',
        notice:
          'Using local gcloud OAuth credentials for Vertex AI publisher models. The access token is short-lived and is not stored by Coddy.',
        quotaProjectId: 'coddy-dev',
      })
      expect(fetcher).not.toHaveBeenCalled()
      expect(gcloudTokenProvider).toHaveBeenCalledOnce()
      expect(gcloudProjectProvider).toHaveBeenCalledOnce()
    } finally {
      if (previousCredentialPath === undefined) {
        delete process.env.GOOGLE_APPLICATION_CREDENTIALS
      } else {
        process.env.GOOGLE_APPLICATION_CREDENTIALS = previousCredentialPath
      }
      rmSync(directory, { recursive: true, force: true })
    }
  })

  it('resolves the active gcloud project for Vertex runtime metadata', async () => {
    const runner = vi.fn((_file, _args, _options, callback) => {
      callback(null, 'coddy-dev\n', '')
    })

    await expect(resolveGcloudProjectId(runner)).resolves.toBe('coddy-dev')
    expect(runner).toHaveBeenCalledWith(
      'gcloud',
      ['config', 'get-value', 'project'],
      expect.objectContaining({
        maxBuffer: 1024,
        timeout: 5_000,
        windowsHide: true,
      }),
      expect.any(Function),
    )
  })

  it('keeps available Vertex publisher models when one partner publisher fails', async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(jsonResponse({ publisherModels: [] }))
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
    expect(result.notices).toEqual([
      'Vertex publisher anthropic listing failed at global: Provider returned 403 Forbidden',
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

  it('uses the requested Azure API version when listing deployments', async () => {
    const fetcher = vi.fn().mockResolvedValue(
      jsonResponse({
        data: [
          {
            id: 'gpt-4.1-coddy',
            model: 'gpt-4.1',
          },
        ],
      }),
    )

    const result = await listProviderModels(
      {
        provider: 'azure',
        apiKey: 'azure-key',
        endpoint: 'https://coddy-resource.openai.azure.com',
        apiVersion: '2025-01-01-preview',
      },
      fetcher,
    )

    expect(result.error).toBeUndefined()
    expect(fetcher).toHaveBeenCalledWith(
      'https://coddy-resource.openai.azure.com/openai/deployments?api-version=2025-01-01-preview',
      expect.objectContaining({
        headers: expect.objectContaining({
          'api-key': 'azure-key',
        }),
      }),
    )
  })
})
