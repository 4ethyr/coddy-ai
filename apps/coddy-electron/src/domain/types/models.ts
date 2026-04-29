import type { ModelRef } from './events'

export type ModelProviderId =
  | 'ollama'
  | 'openai'
  | 'openrouter'
  | 'vertex'
  | 'azure'

export type ProviderConnectionKind =
  | 'local'
  | 'api_key'
  | 'workload_identity'
  | 'azure_resource'

export interface ModelCatalogEntry {
  model: ModelRef
  label: string
  description: string
  tags: readonly string[]
}

export interface ModelProviderListRequest {
  provider: ModelProviderId
  apiKey?: string
  endpoint?: string
  rememberCredential?: boolean
}

export interface ModelProviderListResult {
  provider: ModelProviderId
  models: ModelCatalogEntry[]
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

export interface ModelProviderOption {
  id: ModelProviderId
  label: string
  shortLabel: string
  description: string
  connectionLabel: string
  routingLabel: string
  connectionKind: ProviderConnectionKind
  credentialLabel?: string
  credentialPlaceholder?: string
  endpointLabel?: string
  endpointPlaceholder?: string
  requiresCredential: boolean
  requiresEndpoint?: boolean
  models: readonly ModelCatalogEntry[]
}

export const MODEL_PROVIDER_CATALOG: readonly ModelProviderOption[] = [
  {
    id: 'ollama',
    label: 'Local Ollama',
    shortLabel: 'Ollama',
    description: 'Runtime local para baixa latencia e privacidade.',
    connectionLabel: 'local',
    routingLabel: 'workspace',
    connectionKind: 'local',
    requiresCredential: false,
    models: [],
  },
  {
    id: 'openai',
    label: 'OpenAI',
    shortLabel: 'OpenAI',
    description: 'Modelos via API OpenAI para coding e tool use.',
    connectionLabel: 'api key',
    routingLabel: 'responses',
    connectionKind: 'api_key',
    credentialLabel: 'OpenAI API key',
    credentialPlaceholder: 'sk-...',
    requiresCredential: true,
    models: [],
  },
  {
    id: 'openrouter',
    label: 'OpenRouter',
    shortLabel: 'Router',
    description: 'Roteamento OpenAI-compatible para multiplos vendors.',
    connectionLabel: 'api key',
    routingLabel: 'multi-provider',
    connectionKind: 'api_key',
    credentialLabel: 'OpenRouter API key',
    credentialPlaceholder: 'sk-or-...',
    requiresCredential: true,
    models: [],
  },
  {
    id: 'vertex',
    label: 'Google Vertex',
    shortLabel: 'Vertex',
    description: 'Gemini via API key; Vertex Model Garden e Claude via OAuth.',
    connectionLabel: 'api key/token',
    routingLabel: 'project scoped',
    connectionKind: 'api_key',
    credentialLabel: 'Google API key or OAuth token',
    credentialPlaceholder: 'Google API key ou Bearer token',
    requiresCredential: true,
    models: [],
  },
  {
    id: 'azure',
    label: 'Azure OpenAI',
    shortLabel: 'Azure',
    description: 'Deployments Azure OpenAI gerenciados por recurso.',
    connectionLabel: 'resource',
    routingLabel: 'deployment',
    connectionKind: 'azure_resource',
    credentialLabel: 'Azure OpenAI API key',
    credentialPlaceholder: 'api-key',
    endpointLabel: 'Azure endpoint',
    endpointPlaceholder: 'https://resource.openai.azure.com',
    requiresCredential: true,
    requiresEndpoint: true,
    models: [],
  },
]

export function getModelProvider(
  provider: string,
): ModelProviderOption | undefined {
  return MODEL_PROVIDER_CATALOG.find((item) => item.id === provider)
}

export function getModelCatalogEntry(
  model: ModelRef,
): ModelCatalogEntry | undefined {
  return MODEL_PROVIDER_CATALOG.flatMap((provider) => provider.models).find(
    (entry) =>
      entry.model.provider === model.provider && entry.model.name === model.name,
  )
}
