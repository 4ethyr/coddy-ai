import { describe, expect, it, vi } from 'vitest'
import {
  CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV,
  buildRuntimeCredentialEnvironment,
} from '../../main/runtimeCredentialBridge'

describe('runtimeCredentialBridge', () => {
  it('does not build credential environment for local providers', async () => {
    const store = { get: vi.fn() }

    await expect(
      buildRuntimeCredentialEnvironment(
        { provider: 'ollama', name: 'qwen2.5' },
        store,
      ),
    ).resolves.toEqual({})
    expect(store.get).not.toHaveBeenCalled()
  })

  it('builds a redaction-safe env payload from secure stored credentials', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'sk-test',
        endpoint: 'https://api.openai.com/v1',
      }),
    }

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'openai', name: 'gpt-test' },
      store,
    )

    expect(Object.keys(env)).toEqual([CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV])
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'openai',
      token: 'sk-test',
      endpoint: 'https://api.openai.com/v1',
    })
  })

  it('forwards Azure endpoint-scoped credentials without gcloud metadata', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'azure-key',
        endpoint: 'https://coddy-resource.openai.azure.com',
        apiVersion: '2025-01-01-preview',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('unused-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'azure', name: 'gpt-4.1-coddy' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(store.get).toHaveBeenCalledWith('azure')
    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).not.toHaveBeenCalled()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'azure',
      token: 'azure-key',
      endpoint: 'https://coddy-resource.openai.azure.com',
      metadata: {
        api_version: '2025-01-01-preview',
      },
    })
  })

  it('falls back to a short-lived gcloud token for Vertex without storing it', async () => {
    const store = { get: vi.fn().mockResolvedValue(null) }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('ya29.gcloud-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'claude-sonnet-test' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(gcloudTokenProvider).toHaveBeenCalledOnce()
    expect(gcloudProjectProvider).toHaveBeenCalledOnce()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'ya29.gcloud-token',
      metadata: {
        project_id: 'coddy-dev',
      },
    })
  })

  it('adds Vertex project and region metadata for stored OAuth credentials', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'ya29.vertex-token',
        endpoint: 'us-east5',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('unused-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'claude-sonnet-test' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).toHaveBeenCalledOnce()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'ya29.vertex-token',
      endpoint: 'us-east5',
      metadata: {
        project_id: 'coddy-dev',
        region: 'us-east5',
      },
    })
  })

  it('ignores stored Gemini API keys for Vertex Claude and falls back to gcloud OAuth', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'AIza-gemini-key',
        endpoint: 'us-east5',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('ya29.gcloud-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'claude-opus-4-5@20251101' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(gcloudTokenProvider).toHaveBeenCalledOnce()
    expect(gcloudProjectProvider).toHaveBeenCalledOnce()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'ya29.gcloud-token',
      endpoint: 'us-east5',
      metadata: {
        project_id: 'coddy-dev',
        region: 'us-east5',
      },
    })
  })

  it('does not resolve gcloud project metadata for Gemini API-key runtime calls', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'AIza-gemini-key',
        endpoint: 'us-east5',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('unused-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'gemini-2.5-flash' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).not.toHaveBeenCalled()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'AIza-gemini-key',
      endpoint: 'us-east5',
    })
  })

  it('falls back to gcloud OAuth for Vertex Gemini when no API key is stored', async () => {
    const store = { get: vi.fn().mockResolvedValue(null) }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('ya29.gcloud-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'gemini-2.5-flash' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(gcloudTokenProvider).toHaveBeenCalledOnce()
    expect(gcloudProjectProvider).toHaveBeenCalledOnce()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'ya29.gcloud-token',
      metadata: {
        project_id: 'coddy-dev',
      },
    })
  })

  it('adds Vertex metadata for stored OAuth credentials on Vertex Gemini', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'ya29.vertex-token',
        endpoint: 'us-central1',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('unused-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'gemini-2.5-flash' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).toHaveBeenCalledOnce()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'ya29.vertex-token',
      endpoint: 'us-central1',
      metadata: {
        project_id: 'coddy-dev',
        region: 'us-central1',
      },
    })
  })

  it('returns an empty environment when no cloud credential is available', async () => {
    const store = { get: vi.fn().mockResolvedValue(null) }
    const gcloudTokenProvider = vi.fn().mockResolvedValue(null)
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    await expect(
      buildRuntimeCredentialEnvironment(
        { provider: 'openrouter', name: 'anthropic/claude' },
        store,
        gcloudTokenProvider,
        gcloudProjectProvider,
      ),
    ).resolves.toEqual({})
    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).not.toHaveBeenCalled()
  })

  it('forwards stored OpenRouter API keys to the runtime without gcloud metadata', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'sk-or-test',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('unused-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'openrouter', name: 'anthropic/claude-sonnet-4.5' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(store.get).toHaveBeenCalledWith('openrouter')
    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).not.toHaveBeenCalled()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'openrouter',
      token: 'sk-or-test',
    })
  })

  it('forwards stored NVIDIA API keys to the runtime without gcloud metadata', async () => {
    const store = {
      get: vi.fn().mockResolvedValue({
        apiKey: 'nvapi-test',
      }),
    }
    const gcloudTokenProvider = vi.fn().mockResolvedValue('unused-token')
    const gcloudProjectProvider = vi.fn().mockResolvedValue('coddy-dev')

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'nvidia', name: 'deepseek-ai/deepseek-v4-pro' },
      store,
      gcloudTokenProvider,
      gcloudProjectProvider,
    )

    expect(store.get).toHaveBeenCalledWith('nvidia')
    expect(gcloudTokenProvider).not.toHaveBeenCalled()
    expect(gcloudProjectProvider).not.toHaveBeenCalled()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'nvidia',
      token: 'nvapi-test',
    })
  })
})
