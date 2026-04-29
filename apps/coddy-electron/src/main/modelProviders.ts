type ModelProviderId =
  | 'ollama'
  | 'openai'
  | 'openrouter'
  | 'vertex'
  | 'azure'

type Fetcher = (
  url: string,
  init?: {
    headers?: Record<string, string>
    signal?: AbortSignal
  },
) => Promise<{
  ok: boolean
  status: number
  statusText: string
  json(): Promise<unknown>
}>

export interface ModelCatalogEntryPayload {
  model: {
    provider: ModelProviderId
    name: string
  }
  label: string
  description: string
  tags: readonly string[]
}

export interface ModelProviderListPayload {
  provider: ModelProviderId
  apiKey?: string
  endpoint?: string
}

export interface ModelProviderListPayloadResult {
  provider: ModelProviderId
  models: ModelCatalogEntryPayload[]
  source: 'api' | 'local'
  fetchedAtUnixMs: number
  error?: {
    code: string
    message: string
  }
}

const MODEL_LIST_TIMEOUT_MS = 12_000
const MAX_MODELS_PER_PROVIDER = 500

export async function listProviderModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher = fetch,
): Promise<ModelProviderListPayloadResult> {
  if (!isModelProviderId(request.provider)) {
    return errorResult('ollama', 'INVALID_PROVIDER', 'Unsupported provider.')
  }

  try {
    switch (request.provider) {
      case 'ollama':
        return await listOllamaModels(fetcher)
      case 'openai':
        return await listOpenAiModels(request, fetcher)
      case 'openrouter':
        return await listOpenRouterModels(request, fetcher)
      case 'vertex':
        return await listGoogleModels(request, fetcher)
      case 'azure':
        return await listAzureModels(request, fetcher)
    }
  } catch (error) {
    return errorResult(
      request.provider,
      'MODEL_LIST_FAILED',
      error instanceof Error ? error.message : 'Unable to list models.',
    )
  }
}

async function listOllamaModels(
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const data = await fetchJson(
    'http://127.0.0.1:11434/api/tags',
    {},
    fetcher,
  )
  const models = asArray(getObject(data).models)
    .map((item) => {
      const object = getObject(item)
      const name = getString(object.name)
      if (!name) return null
      const details = getObject(object.details)
      const family = getString(details.family)
      return modelEntry('ollama', name, name, family || 'Local Ollama model.', [
        'local',
        family,
      ])
    })
    .filter((item): item is ModelCatalogEntryPayload => Boolean(item))

  return successResult('ollama', 'local', models)
}

async function listOpenAiModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const apiKey = requireCredential(request)
  const data = await fetchJson(
    'https://api.openai.com/v1/models',
    {
      Authorization: `Bearer ${apiKey}`,
    },
    fetcher,
  )
  const models = asArray(getObject(data).data)
    .map((item) => {
      const object = getObject(item)
      const id = getString(object.id)
      if (!id) return null
      const owner = getString(object.owned_by)
      return modelEntry(
        'openai',
        id,
        id,
        owner ? `OpenAI model owned by ${owner}.` : 'OpenAI API model.',
        ['api', owner],
      )
    })
    .filter((item): item is ModelCatalogEntryPayload => Boolean(item))

  return successResult('openai', 'api', models)
}

async function listOpenRouterModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const apiKey = requireCredential(request)
  const data = await fetchJson(
    'https://openrouter.ai/api/v1/models?output_modalities=text',
    {
      Authorization: `Bearer ${apiKey}`,
    },
    fetcher,
  )
  const models = asArray(getObject(data).data)
    .map((item) => {
      const object = getObject(item)
      const id = getString(object.id)
      if (!id) return null
      const label = getString(object.name) || id
      const description =
        getString(object.description) || 'OpenRouter model endpoint.'
      const contextLength = getNumber(object.context_length)
      const tags = ['api', contextLength ? `${contextLength} ctx` : undefined]
      return modelEntry('openrouter', id, label, description, tags)
    })
    .filter((item): item is ModelCatalogEntryPayload => Boolean(item))

  return successResult('openrouter', 'api', models)
}

async function listGoogleModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const credential = requireCredential(request)
  if (looksLikeGoogleApiKey(credential)) {
    return listGeminiApiModels(credential, fetcher)
  }
  return listVertexPublisherModels(stripBearerPrefix(credential), fetcher)
}

async function listGeminiApiModels(
  apiKey: string,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const data = await fetchJson(
    'https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000',
    {
      'x-goog-api-key': apiKey,
    },
    fetcher,
  )
  const models = asArray(getObject(data).models)
    .map((item) => {
      const object = getObject(item)
      const id =
        getString(object.baseModelId) ||
        lastResourceSegment(getString(object.name))
      if (!id) return null
      const label = getString(object.displayName) || id
      const description =
        getString(object.description) || 'Google Gemini API model.'
      const supportedActions = asArray(object.supportedActions)
        .map((value) => getString(value))
        .filter(Boolean)
      return modelEntry('vertex', id, label, description, [
        'api',
        ...supportedActions,
      ])
    })
    .filter((item): item is ModelCatalogEntryPayload => Boolean(item))

  return successResult('vertex', 'api', models)
}

async function listVertexPublisherModels(
  token: string,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const data = await fetchJson(
    'https://aiplatform.googleapis.com/v1beta1/publishers/google/models?pageSize=100&view=BASIC',
    {
      Authorization: `Bearer ${token}`,
    },
    fetcher,
  )
  const models = asArray(getObject(data).publisherModels)
    .map((item) => {
      const object = getObject(item)
      const id = lastResourceSegment(getString(object.name))
      if (!id) return null
      const launchStage = getString(object.launchStage)
      const versionId = getString(object.versionId)
      return modelEntry(
        'vertex',
        id,
        id,
        versionId ? `Vertex publisher model version ${versionId}.` : 'Vertex publisher model.',
        ['vertex', launchStage],
      )
    })
    .filter((item): item is ModelCatalogEntryPayload => Boolean(item))

  return successResult('vertex', 'api', models)
}

async function listAzureModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const apiKey = requireCredential(request)
  const endpoint = normalizeHttpsEndpoint(request.endpoint)
  const data = await fetchJson(
    `${endpoint}/openai/deployments?api-version=2024-10-21`,
    {
      'api-key': apiKey,
    },
    fetcher,
  )
  const models = getAzureModelItems(data)
    .map((item) => {
      const object = getObject(item)
      const id =
        getString(object.id) ||
        getString(object.name) ||
        lastResourceSegment(getString(object.model))
      if (!id) return null
      const modelName = getString(object.model)
      const label = getString(object.displayName) || id
      return modelEntry(
        'azure',
        id,
        label,
        modelName ? `Azure deployment for ${modelName}.` : 'Azure OpenAI deployment.',
        ['deployment', modelName],
      )
    })
    .filter((item): item is ModelCatalogEntryPayload => Boolean(item))

  return successResult('azure', 'api', models)
}

async function fetchJson(
  url: string,
  headers: Record<string, string>,
  fetcher: Fetcher,
): Promise<unknown> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), MODEL_LIST_TIMEOUT_MS)
  try {
    const response = await fetcher(url, {
      headers: {
        Accept: 'application/json',
        ...headers,
      },
      signal: controller.signal,
    })

    if (!response.ok) {
      throw new Error(
        `Provider returned ${response.status} ${response.statusText}`.trim(),
      )
    }

    return await response.json()
  } finally {
    clearTimeout(timeout)
  }
}

function requireCredential(request: ModelProviderListPayload): string {
  const value = request.apiKey?.trim()
  if (!value) {
    throw new Error('Provider credential is required.')
  }
  return value
}

function normalizeHttpsEndpoint(endpoint: string | undefined): string {
  const value = endpoint?.trim()
  if (!value) {
    throw new Error('Provider endpoint is required.')
  }

  const parsed = new URL(value)
  if (parsed.protocol !== 'https:') {
    throw new Error('Provider endpoint must use HTTPS.')
  }
  parsed.pathname = parsed.pathname.replace(/\/+$/, '')
  parsed.search = ''
  parsed.hash = ''
  return parsed.toString().replace(/\/$/, '')
}

function successResult(
  provider: ModelProviderId,
  source: 'api' | 'local',
  models: ModelCatalogEntryPayload[],
): ModelProviderListPayloadResult {
  return {
    provider,
    source,
    fetchedAtUnixMs: Date.now(),
    models: normalizeModelList(models),
  }
}

function errorResult(
  provider: ModelProviderId,
  code: string,
  message: string,
): ModelProviderListPayloadResult {
  return {
    provider,
    source: provider === 'ollama' ? 'local' : 'api',
    fetchedAtUnixMs: Date.now(),
    models: [],
    error: { code, message },
  }
}

function normalizeModelList(
  models: ModelCatalogEntryPayload[],
): ModelCatalogEntryPayload[] {
  const seen = new Set<string>()
  return models
    .filter((item) => {
      const key = `${item.model.provider}/${item.model.name}`
      if (seen.has(key)) return false
      seen.add(key)
      return true
    })
    .sort((left, right) => left.label.localeCompare(right.label))
    .slice(0, MAX_MODELS_PER_PROVIDER)
}

function modelEntry(
  provider: ModelProviderId,
  id: string,
  label: string,
  description: string,
  tags: readonly (string | undefined)[],
): ModelCatalogEntryPayload {
  return {
    model: { provider, name: id },
    label,
    description,
    tags: tags.filter((tag): tag is string => Boolean(tag)),
  }
}

function getAzureModelItems(data: unknown): unknown[] {
  const object = getObject(data)
  const candidates = [
    asArray(object.data),
    asArray(object.deployments),
    asArray(object.value),
  ]
  return candidates.find((items) => items.length > 0) ?? []
}

function getObject(value: unknown): Record<string, unknown> {
  if (value && typeof value === 'object') {
    return value as Record<string, unknown>
  }
  return {}
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

function getString(value: unknown): string {
  return typeof value === 'string' ? value.trim() : ''
}

function getNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function lastResourceSegment(value: string): string {
  const normalized = value.trim().replace(/\/+$/, '')
  return normalized ? normalized.split('/').pop() ?? '' : ''
}

function stripBearerPrefix(value: string): string {
  return value.replace(/^Bearer\s+/i, '').trim()
}

function looksLikeGoogleApiKey(value: string): boolean {
  return value.startsWith('AIza')
}

function isModelProviderId(value: string): value is ModelProviderId {
  return (
    value === 'ollama' ||
    value === 'openai' ||
    value === 'openrouter' ||
    value === 'vertex' ||
    value === 'azure'
  )
}
