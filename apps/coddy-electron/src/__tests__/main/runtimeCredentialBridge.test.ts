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

    const env = await buildRuntimeCredentialEnvironment(
      { provider: 'vertex', name: 'claude-sonnet-test' },
      store,
      gcloudTokenProvider,
    )

    expect(gcloudTokenProvider).toHaveBeenCalledOnce()
    expect(JSON.parse(env[CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV] ?? '{}')).toEqual({
      provider: 'vertex',
      token: 'ya29.gcloud-token',
    })
  })

  it('returns an empty environment when no cloud credential is available', async () => {
    const store = { get: vi.fn().mockResolvedValue(null) }
    const gcloudTokenProvider = vi.fn().mockResolvedValue(null)

    await expect(
      buildRuntimeCredentialEnvironment(
        { provider: 'openrouter', name: 'anthropic/claude' },
        store,
        gcloudTokenProvider,
      ),
    ).resolves.toEqual({})
    expect(gcloudTokenProvider).not.toHaveBeenCalled()
  })
})
