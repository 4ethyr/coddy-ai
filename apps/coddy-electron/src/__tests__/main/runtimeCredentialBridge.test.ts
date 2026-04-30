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
})
