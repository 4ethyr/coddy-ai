import type { ModelProviderId } from './modelProviders'
import { resolveGcloudAccessToken } from './modelProviders'
import type { ProviderCredentialRecord } from './secureCredentialStore'

export const CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV =
  'CODDY_EPHEMERAL_MODEL_CREDENTIAL'

export type RuntimeCredentialModelRef = {
  provider: string
  name: string
}

export type RuntimeCredentialStore = {
  get(provider: ModelProviderId): Promise<ProviderCredentialRecord | null>
}

export type GcloudTokenProvider = () => Promise<string | null>

type EphemeralModelCredentialPayload = {
  provider: ModelProviderId
  token: string
  endpoint?: string
}

export async function buildRuntimeCredentialEnvironment(
  model: RuntimeCredentialModelRef,
  credentialStore: RuntimeCredentialStore,
  gcloudTokenProvider: GcloudTokenProvider = resolveGcloudAccessToken,
): Promise<Record<string, string>> {
  const provider = normalizeRuntimeCredentialProvider(model.provider)
  if (!provider || provider === 'ollama') return {}

  const stored = await credentialStore.get(provider)
  const storedToken = stored?.apiKey?.trim()
  const storedEndpoint = stored?.endpoint?.trim()
  if (storedToken) {
    return ephemeralCredentialEnvironment({
      provider,
      token: storedToken,
      ...(storedEndpoint ? { endpoint: storedEndpoint } : {}),
    })
  }

  if (provider !== 'vertex') return {}

  const gcloudToken = await gcloudTokenProvider()
  if (!gcloudToken) return {}

  return ephemeralCredentialEnvironment({
    provider,
    token: gcloudToken,
  })
}

function ephemeralCredentialEnvironment(
  credential: EphemeralModelCredentialPayload,
): Record<string, string> {
  return {
    [CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV]: JSON.stringify(credential),
  }
}

function normalizeRuntimeCredentialProvider(
  provider: string,
): ModelProviderId | null {
  if (
    provider === 'ollama' ||
    provider === 'openai' ||
    provider === 'openrouter' ||
    provider === 'vertex' ||
    provider === 'azure'
  ) {
    return provider
  }
  return null
}
