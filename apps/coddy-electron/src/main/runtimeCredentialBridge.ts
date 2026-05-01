import type { ModelProviderId } from './modelProviders'
import { resolveGcloudAccessToken, resolveGcloudProjectId } from './modelProviders'
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
export type GcloudProjectProvider = () => Promise<string | null>

type EphemeralModelCredentialPayload = {
  provider: ModelProviderId
  token: string
  endpoint?: string
  metadata?: Record<string, string>
}

export async function buildRuntimeCredentialEnvironment(
  model: RuntimeCredentialModelRef,
  credentialStore: RuntimeCredentialStore,
  gcloudTokenProvider: GcloudTokenProvider = resolveGcloudAccessToken,
  gcloudProjectProvider: GcloudProjectProvider = resolveGcloudProjectId,
): Promise<Record<string, string>> {
  const provider = normalizeRuntimeCredentialProvider(model.provider)
  if (!provider || provider === 'ollama') return {}

  const stored = await credentialStore.get(provider)
  const storedToken = stored?.apiKey?.trim()
  const storedEndpoint = stored?.endpoint?.trim()
  const storedApiVersion = stored?.apiVersion?.trim()
  const isVertexRuntime = provider === 'vertex'
  const needsVertexMetadata =
    isVertexRuntime
    && (!storedToken || isGoogleOAuthCredential(storedToken))
  if (storedToken) {
    if (
      isVertexAnthropicRuntimeModel(model.name)
      && !isGoogleOAuthCredential(storedToken)
    ) {
      return buildGcloudVertexCredentialEnvironment(
        provider,
        storedEndpoint,
        storedApiVersion,
        gcloudTokenProvider,
        gcloudProjectProvider,
      )
    }

    const metadata = await runtimeCredentialMetadata(
      provider,
      needsVertexMetadata,
      storedEndpoint,
      storedApiVersion,
      gcloudProjectProvider,
    )
    return ephemeralCredentialEnvironment({
      provider,
      token: storedToken,
      ...(storedEndpoint ? { endpoint: storedEndpoint } : {}),
      ...(metadata ? { metadata } : {}),
    })
  }

  if (provider !== 'vertex') return {}

  return buildGcloudVertexCredentialEnvironment(
    provider,
    storedEndpoint,
    storedApiVersion,
    gcloudTokenProvider,
    gcloudProjectProvider,
  )
}

async function buildGcloudVertexCredentialEnvironment(
  provider: ModelProviderId,
  storedEndpoint: string | undefined,
  storedApiVersion: string | undefined,
  gcloudTokenProvider: GcloudTokenProvider,
  gcloudProjectProvider: GcloudProjectProvider,
): Promise<Record<string, string>> {
  const gcloudToken = await gcloudTokenProvider()
  if (!gcloudToken) return {}
  const metadata = await runtimeCredentialMetadata(
    provider,
    true,
    storedEndpoint,
    storedApiVersion,
    gcloudProjectProvider,
  )

  return ephemeralCredentialEnvironment({
    provider,
    token: gcloudToken,
    ...(storedEndpoint ? { endpoint: storedEndpoint } : {}),
    ...(metadata ? { metadata } : {}),
  })
}

function isGoogleOAuthCredential(value: string): boolean {
  return /^Bearer\s+/i.test(value) || value.startsWith('ya29.')
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

async function runtimeCredentialMetadata(
  provider: ModelProviderId,
  needsVertexMetadata: boolean,
  endpoint: string | undefined,
  apiVersion: string | undefined,
  gcloudProjectProvider: GcloudProjectProvider,
): Promise<Record<string, string> | undefined> {
  if (provider === 'azure') {
    return azureRuntimeMetadata(apiVersion)
  }

  if (needsVertexMetadata) {
    return vertexRuntimeMetadata(endpoint, gcloudProjectProvider)
  }

  return undefined
}

function azureRuntimeMetadata(
  apiVersion: string | undefined,
): Record<string, string> | undefined {
  const value = apiVersion?.trim()
  return value ? { api_version: value } : undefined
}

async function vertexRuntimeMetadata(
  endpoint: string | undefined,
  gcloudProjectProvider: GcloudProjectProvider,
): Promise<Record<string, string> | undefined> {
  const metadata: Record<string, string> = {}
  const projectId = await gcloudProjectProvider()
  if (projectId) {
    metadata.project_id = projectId
  }

  const region = vertexRegionFromEndpoint(endpoint)
  if (region) {
    metadata.region = region
  }

  return Object.keys(metadata).length > 0 ? metadata : undefined
}

function vertexRegionFromEndpoint(endpoint: string | undefined): string | null {
  const value = endpoint?.trim()
  if (!value || value.startsWith('https://')) return null
  return /^[a-z0-9-]+$/i.test(value) ? value : null
}

function isVertexAnthropicRuntimeModel(model: string): boolean {
  return model.trim().startsWith('claude-')
}
