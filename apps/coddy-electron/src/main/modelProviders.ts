import { createSign } from 'crypto'
import { execFile as nodeExecFile } from 'child_process'
import { promises as fs } from 'fs'
import * as os from 'os'
import * as path from 'path'

export type ModelProviderId =
  | 'ollama'
  | 'openai'
  | 'openrouter'
  | 'vertex'
  | 'azure'

type Fetcher = (
  url: string,
  init?: {
    headers?: Record<string, string>
    method?: string
    body?: string
    signal?: AbortSignal
  },
) => Promise<{
  ok: boolean
  status: number
  statusText: string
  json(): Promise<unknown>
}>

type ExecFileCallback = (
  error: Error | null,
  stdout: string | Buffer,
  stderr: string | Buffer,
) => void

type ExecFileRunner = (
  file: string,
  args: string[],
  options: {
    timeout: number
    windowsHide: boolean
    maxBuffer: number
  },
  callback: ExecFileCallback,
) => void

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
  apiVersion?: string
  rememberCredential?: boolean
}

export interface ModelProviderListPayloadResult {
  provider: ModelProviderId
  models: ModelCatalogEntryPayload[]
  source: 'api' | 'local'
  fetchedAtUnixMs: number
  notices?: string[]
  error?: {
    code: string
    message: string
  }
  credentialStorage?: {
    persisted: boolean
    message: string
  }
}

type GoogleAccessTokenResolution =
  | string
  | {
      token: string
      notice: string
      quotaProjectId?: string
    }

type GoogleAccessTokenProvider = (
  fetcher: Fetcher,
) => Promise<GoogleAccessTokenResolution | null>

const MODEL_LIST_TIMEOUT_MS = 12_000
const MAX_MODELS_PER_PROVIDER = 500
const GCLOUD_TOKEN_TIMEOUT_MS = 10_000
const GCLOUD_TOKEN_MAX_BUFFER_BYTES = 4_096
const GCLOUD_PROJECT_TIMEOUT_MS = 5_000
const GCLOUD_PROJECT_MAX_BUFFER_BYTES = 1_024
const GOOGLE_AUTH_SCOPE = 'https://www.googleapis.com/auth/cloud-platform'
const GOOGLE_JWT_BEARER_GRANT =
  'urn:ietf:params:oauth:grant-type:jwt-bearer'
const GEMINI_API_KEY_NOTICE =
  'Gemini API keys list Gemini models only. Claude on Vertex requires a Google OAuth access token or Application Default Credentials.'
const VERTEX_ADC_NOTICE =
  'Using Google Application Default Credentials for Vertex AI publisher models.'
const VERTEX_GCLOUD_NOTICE =
  'Using local gcloud OAuth credentials for Vertex AI publisher models. The access token is short-lived and is not stored by Coddy.'
const DEFAULT_AZURE_API_VERSION = '2024-10-21'
const VERTEX_PUBLISHERS = [
  {
    id: 'google',
    defaultEndpoints: ['https://aiplatform.googleapis.com'],
  },
  {
    id: 'anthropic',
    defaultEndpoints: [
      'https://aiplatform.googleapis.com',
      'https://us-east5-aiplatform.googleapis.com',
      'https://europe-west1-aiplatform.googleapis.com',
    ],
  },
] as const
const VERTEX_PUBLISHER_PAGE_SIZE = 200

export async function listProviderModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher = fetch,
  googleAccessTokenProvider: GoogleAccessTokenProvider =
    resolveGoogleApplicationDefaultAccessToken,
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
        return await listGoogleModels(request, fetcher, googleAccessTokenProvider)
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
  googleAccessTokenProvider: GoogleAccessTokenProvider,
): Promise<ModelProviderListPayloadResult> {
  const credential = request.apiKey?.trim()
  if (looksLikeGoogleOAuthCredential(credential)) {
    return listVertexPublisherModels(
      stripBearerPrefix(credential),
      fetcher,
      [],
      request.endpoint,
    )
  }
  if (!credential) {
    const resolvedToken = normalizeGoogleAccessTokenResolution(
      await googleAccessTokenProvider(fetcher),
    )
    if (resolvedToken) {
      return listVertexPublisherModels(
        resolvedToken.token,
        fetcher,
        [resolvedToken.notice],
        request.endpoint,
        resolvedToken.quotaProjectId,
      )
    }
    throw new Error(
      'Provider credential is required. Use a Google API key for Gemini, paste a Vertex OAuth Bearer token for Claude, set GOOGLE_APPLICATION_CREDENTIALS, run gcloud auth application-default login, or run gcloud auth login.',
    )
  }
  return listGeminiApiModels(credential, fetcher)
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

  return successResult('vertex', 'api', models, [GEMINI_API_KEY_NOTICE])
}

async function listVertexPublisherModels(
  token: string,
  fetcher: Fetcher,
  notices: string[] = [],
  endpoint: string | undefined = undefined,
  quotaProjectId: string | undefined = undefined,
): Promise<ModelProviderListPayloadResult> {
  const endpointOverride = normalizeVertexPublisherEndpoint(endpoint)
  const tasks = VERTEX_PUBLISHERS.flatMap((publisher) =>
    vertexPublisherEndpoints(publisher, endpointOverride).map((publisherEndpoint) => ({
      publisher,
      endpoint: publisherEndpoint,
      promise: listVertexPublisherModelGroup(
        publisher,
        token,
        fetcher,
        publisherEndpoint,
        quotaProjectId,
      ),
    })),
  )
  const modelGroups = await Promise.allSettled(
    tasks.map((task) => task.promise),
  )
  const models = modelGroups.flatMap((result) =>
    result.status === 'fulfilled' ? result.value : [],
  )
  if (models.length === 0) {
    const firstError = modelGroups.find(
      (result): result is PromiseRejectedResult => result.status === 'rejected',
    )
    throw firstError?.reason instanceof Error
      ? firstError.reason
      : new Error('Unable to list Vertex publisher models.')
  }
  const failureNotices = modelGroups
    .map((result, index) => {
      if (result.status !== 'rejected') return null
      const task = tasks[index]
      if (!task) return null
      return `Vertex publisher ${task.publisher.id} listing failed at ${describeVertexEndpoint(task.endpoint)}: ${errorMessage(result.reason)}`
    })
    .filter((notice): notice is string => Boolean(notice))

  return successResult('vertex', 'api', models, [...notices, ...failureNotices])
}

async function listVertexPublisherModelGroup(
  publisher: (typeof VERTEX_PUBLISHERS)[number],
  token: string,
  fetcher: Fetcher,
  endpoint: string,
  quotaProjectId: string | undefined,
): Promise<ModelCatalogEntryPayload[]> {
  const models: ModelCatalogEntryPayload[] = []
  let pageToken = ''

  do {
    const query = new URLSearchParams({
      pageSize: String(VERTEX_PUBLISHER_PAGE_SIZE),
      listAllVersions: 'true',
    })
    if (pageToken) query.set('pageToken', pageToken)

    const data = await fetchJson(
      `${endpoint}/v1beta1/publishers/${publisher.id}/models?${query.toString()}`,
      {
        Authorization: `Bearer ${token}`,
        ...(quotaProjectId ? { 'x-goog-user-project': quotaProjectId } : {}),
      },
      fetcher,
    )
    const object = getObject(data)
    models.push(
      ...asArray(object.publisherModels)
        .map((item) => {
          const itemObject = getObject(item)
          const id = lastResourceSegment(getString(itemObject.name))
          if (!id) return null
          const launchStage = getString(itemObject.launchStage)
          const versionId = getString(itemObject.versionId)
          const label = getString(itemObject.displayName) || id
          return modelEntry(
            'vertex',
            id,
            label,
            versionId
              ? `Vertex ${publisher.id} model version ${versionId}.`
              : `Vertex ${publisher.id} publisher model.`,
            ['vertex', publisher.id, launchStage],
          )
        })
        .filter((item): item is ModelCatalogEntryPayload => Boolean(item)),
    )
    pageToken = getString(object.nextPageToken) || getString(object.next_page_token)
  } while (pageToken && models.length < MAX_MODELS_PER_PROVIDER)

  return models
}

function vertexPublisherEndpoints(
  publisher: (typeof VERTEX_PUBLISHERS)[number],
  endpointOverride: string | null,
): readonly string[] {
  if (endpointOverride) return [endpointOverride]
  return publisher.defaultEndpoints
}

function describeVertexEndpoint(endpoint: string): string {
  const parsed = new URL(endpoint)
  if (parsed.hostname === 'aiplatform.googleapis.com') return 'global'
  return parsed.hostname.replace(/-aiplatform\.googleapis\.com$/i, '')
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unknown error'
}

function providerErrorMessage(data: unknown): string {
  const object = getObject(data)
  const error = getObject(object.error)
  return getString(error.message) || getString(object.message)
}

async function listAzureModels(
  request: ModelProviderListPayload,
  fetcher: Fetcher,
): Promise<ModelProviderListPayloadResult> {
  const apiKey = requireCredential(request)
  const endpoint = normalizeHttpsEndpoint(request.endpoint)
  const apiVersion = normalizeAzureApiVersion(request.apiVersion)
  const data = await fetchJson(
    `${endpoint}/openai/deployments?api-version=${apiVersion}`,
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
  init: { method?: string; body?: string } = {},
): Promise<unknown> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), MODEL_LIST_TIMEOUT_MS)
  try {
    const response = await fetcher(url, {
      headers: {
        Accept: 'application/json',
        ...headers,
      },
      ...init,
      signal: controller.signal,
    })

    if (!response.ok) {
      const data = await response.json().catch(() => ({}))
      const message = providerErrorMessage(data)
      throw new Error(
        [
          `Provider returned ${response.status} ${response.statusText}`.trim(),
          message,
        ]
          .filter(Boolean)
          .join(': '),
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

function normalizeAzureApiVersion(apiVersion: string | undefined): string {
  const value = apiVersion?.trim() || DEFAULT_AZURE_API_VERSION
  if (
    !value ||
    value
      .split('')
      .some((character) => /\s/.test(character) || ['&', '?', '#'].includes(character))
  ) {
    throw new Error('Azure API version is invalid.')
  }
  return value
}

function normalizeVertexPublisherEndpoint(endpoint: string | undefined): string | null {
  const value = endpoint?.trim()
  if (!value) return null

  if (/^https:\/\//i.test(value)) {
    return normalizeHttpsEndpoint(value)
  }

  if (!/^[a-z0-9-]+$/i.test(value)) {
    throw new Error('Vertex region must be a region id like us-east5 or an HTTPS endpoint.')
  }

  if (value === 'global') {
    return 'https://aiplatform.googleapis.com'
  }

  return `https://${value.toLowerCase()}-aiplatform.googleapis.com`
}

function successResult(
  provider: ModelProviderId,
  source: 'api' | 'local',
  models: ModelCatalogEntryPayload[],
  notices?: string[],
): ModelProviderListPayloadResult {
  return {
    provider,
    source,
    fetchedAtUnixMs: Date.now(),
    models: normalizeModelList(models),
    ...(notices && notices.length > 0 ? { notices } : {}),
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

function looksLikeGoogleOAuthCredential(value: string | undefined): value is string {
  const trimmed = value?.trim() ?? ''
  return /^Bearer\s+/i.test(trimmed) || trimmed.startsWith('ya29.')
}

export async function resolveGoogleApplicationDefaultAccessToken(
  fetcher: Fetcher,
  gcloudAccessTokenProvider: () => Promise<string | null> = resolveGcloudAccessToken,
  gcloudProjectProvider: () => Promise<string | null> = resolveGcloudProjectId,
): Promise<GoogleAccessTokenResolution | null> {
  const gcloudToken = await gcloudAccessTokenProvider()
  if (gcloudToken) {
    const quotaProjectId = (await gcloudProjectProvider()) ?? undefined
    return {
      token: gcloudToken,
      notice: VERTEX_GCLOUD_NOTICE,
      ...(quotaProjectId ? { quotaProjectId } : {}),
    }
  }

  let adcError: string | null = null
  try {
    const adcToken = await resolveGoogleAdcAccessToken(fetcher)
    if (adcToken) {
      const quotaProjectId =
        adcToken.quotaProjectId ?? (await gcloudProjectProvider()) ?? undefined
      return {
        token: adcToken.token,
        notice: VERTEX_ADC_NOTICE,
        ...(quotaProjectId ? { quotaProjectId } : {}),
      }
    }
  } catch (error) {
    adcError = errorMessage(error)
  }

  if (adcError) {
    throw new Error(
      `Google ADC credential was found but could not be refreshed: ${adcError}`,
    )
  }

  return null
}

async function resolveGoogleAdcAccessToken(
  fetcher: Fetcher,
): Promise<{ token: string; quotaProjectId?: string } | null> {
  const credentialPath = googleApplicationCredentialsPath()
  if (!credentialPath) return null

  let raw = ''
  try {
    raw = await fs.readFile(credentialPath, 'utf8')
  } catch {
    return null
  }

  const credential = getObject(JSON.parse(raw) as unknown)
  const type = getString(credential.type)
  const quotaProjectId = getString(credential.quota_project_id)
  const withQuotaProject = (token: string) => ({
    token,
    ...(quotaProjectId ? { quotaProjectId } : {}),
  })
  if (type === 'service_account') {
    return withQuotaProject(await serviceAccountAccessToken(credential, fetcher))
  }
  if (type === 'authorized_user') {
    return withQuotaProject(await authorizedUserAccessToken(credential, fetcher))
  }
  throw new Error('Unsupported Google ADC credential type.')
}

export async function resolveGcloudAccessToken(
  runner: ExecFileRunner = nodeExecFile as ExecFileRunner,
): Promise<string | null> {
  return new Promise((resolve) => {
    runner(
      'gcloud',
      ['auth', 'print-access-token'],
      {
        maxBuffer: GCLOUD_TOKEN_MAX_BUFFER_BYTES,
        timeout: GCLOUD_TOKEN_TIMEOUT_MS,
        windowsHide: true,
      },
      (error, stdout) => {
        if (error) {
          resolve(null)
          return
        }
        resolve(normalizeGcloudTokenOutput(stdout))
      },
    )
  })
}

export async function resolveGcloudProjectId(
  runner: ExecFileRunner = nodeExecFile as ExecFileRunner,
): Promise<string | null> {
  return new Promise((resolve) => {
    runner(
      'gcloud',
      ['config', 'get-value', 'project'],
      {
        maxBuffer: GCLOUD_PROJECT_MAX_BUFFER_BYTES,
        timeout: GCLOUD_PROJECT_TIMEOUT_MS,
        windowsHide: true,
      },
      (error, stdout) => {
        if (error) {
          resolve(null)
          return
        }
        resolve(normalizeGcloudProjectOutput(stdout))
      },
    )
  })
}

function googleApplicationCredentialsPath(): string | null {
  const explicit = process.env.GOOGLE_APPLICATION_CREDENTIALS?.trim()
  if (explicit) return explicit

  const home = os.homedir()
  return home
    ? path.join(home, '.config', 'gcloud', 'application_default_credentials.json')
    : null
}

async function serviceAccountAccessToken(
  credential: Record<string, unknown>,
  fetcher: Fetcher,
): Promise<string> {
  const clientEmail = requiredCredentialField(credential, 'client_email')
  const privateKey = requiredCredentialField(credential, 'private_key')
  const tokenUri =
    getString(credential.token_uri) || 'https://oauth2.googleapis.com/token'
  const assertion = signGoogleServiceAccountJwt(
    clientEmail,
    privateKey,
    tokenUri,
  )
  const data = await fetchJson(
    tokenUri,
    { 'Content-Type': 'application/x-www-form-urlencoded' },
    fetcher,
    {
      method: 'POST',
      body: new URLSearchParams({
        grant_type: GOOGLE_JWT_BEARER_GRANT,
        assertion,
      }).toString(),
    },
  )
  return accessTokenFromTokenResponse(data)
}

async function authorizedUserAccessToken(
  credential: Record<string, unknown>,
  fetcher: Fetcher,
): Promise<string> {
  const clientId = requiredCredentialField(credential, 'client_id')
  const clientSecret = requiredCredentialField(credential, 'client_secret')
  const refreshToken = requiredCredentialField(credential, 'refresh_token')
  const tokenUri =
    getString(credential.token_uri) || 'https://oauth2.googleapis.com/token'
  const data = await fetchJson(
    tokenUri,
    { 'Content-Type': 'application/x-www-form-urlencoded' },
    fetcher,
    {
      method: 'POST',
      body: new URLSearchParams({
        grant_type: 'refresh_token',
        client_id: clientId,
        client_secret: clientSecret,
        refresh_token: refreshToken,
      }).toString(),
    },
  )
  return accessTokenFromTokenResponse(data)
}

function signGoogleServiceAccountJwt(
  clientEmail: string,
  privateKey: string,
  tokenUri: string,
): string {
  const now = Math.floor(Date.now() / 1000)
  const header = base64UrlJson({ alg: 'RS256', typ: 'JWT' })
  const claim = base64UrlJson({
    iss: clientEmail,
    scope: GOOGLE_AUTH_SCOPE,
    aud: tokenUri,
    iat: now,
    exp: now + 3_600,
  })
  const unsigned = `${header}.${claim}`
  const signature = createSign('RSA-SHA256')
    .update(unsigned)
    .end()
    .sign(privateKey)
    .toString('base64url')
  return `${unsigned}.${signature}`
}

function base64UrlJson(value: Record<string, unknown>): string {
  return Buffer.from(JSON.stringify(value)).toString('base64url')
}

function accessTokenFromTokenResponse(data: unknown): string {
  const token = getString(getObject(data).access_token)
  if (!token) {
    throw new Error('Google OAuth token response did not include access_token.')
  }
  return token
}

function normalizeGoogleAccessTokenResolution(
  value: GoogleAccessTokenResolution | null,
): { token: string; notice: string; quotaProjectId?: string } | null {
  if (!value) return null

  if (typeof value === 'string') {
    const token = value.trim()
    return token ? { token, notice: VERTEX_ADC_NOTICE } : null
  }

  const token = value.token.trim()
  const notice = value.notice.trim() || VERTEX_ADC_NOTICE
  const quotaProjectId = value.quotaProjectId?.trim()
  return token
    ? { token, notice, ...(quotaProjectId ? { quotaProjectId } : {}) }
    : null
}

function normalizeGcloudTokenOutput(value: string | Buffer): string | null {
  const token = value.toString('utf8').trim().split(/\s+/)[0] ?? ''
  return token ? token : null
}

function normalizeGcloudProjectOutput(value: string | Buffer): string | null {
  const projectId = value.toString('utf8').trim().split(/\s+/)[0] ?? ''
  if (!projectId || projectId === '(unset)') return null
  return projectId
}

function requiredCredentialField(
  credential: Record<string, unknown>,
  field: string,
): string {
  const value = getString(credential[field])
  if (!value) {
    throw new Error(`Google ADC credential is missing ${field}.`)
  }
  return value
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
