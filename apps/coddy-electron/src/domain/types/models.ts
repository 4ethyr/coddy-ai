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

export type RuntimeChatSupport = 'supported' | 'adapter_pending'

export interface RuntimeChatCapability {
  status: RuntimeChatSupport
  label: string
  description: string
}

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
  allowsLocalCredential?: boolean
  requiresEndpoint?: boolean
  runtimeChat: RuntimeChatCapability
  models: readonly ModelCatalogEntry[]
}

const LOCAL_RUNTIME_CHAT: RuntimeChatCapability = {
  status: 'supported',
  label: 'runtime ready',
  description:
    'Chat execution, streaming events and safe low-risk tools are wired through the Rust runtime.',
}

const CLOUD_DISCOVERY_ONLY_CHAT: RuntimeChatCapability = {
  status: 'adapter_pending',
  label: 'adapter pending',
  description:
    'Model discovery and selection are wired; chat execution still requires a Rust runtime adapter for this provider.',
}

const UNKNOWN_RUNTIME_CHAT: RuntimeChatCapability = {
  status: 'adapter_pending',
  label: 'adapter pending',
  description:
    'This provider is not registered in the frontend model catalog yet.',
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
    runtimeChat: LOCAL_RUNTIME_CHAT,
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
    runtimeChat: CLOUD_DISCOVERY_ONLY_CHAT,
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
    runtimeChat: CLOUD_DISCOVERY_ONLY_CHAT,
    models: [],
  },
  {
    id: 'vertex',
    label: 'Google Vertex',
    shortLabel: 'Vertex',
    description: 'Gemini via API key; Vertex Model Garden e Claude via OAuth/ADC/gcloud.',
    connectionLabel: 'api key/oauth',
    routingLabel: 'project scoped',
    connectionKind: 'api_key',
    credentialLabel: 'Google API key, OAuth token, ADC or gcloud',
    credentialPlaceholder: 'API key, Bearer token, or leave blank for gcloud',
    endpointLabel: 'Vertex region',
    endpointPlaceholder: 'global, us-east5 ou https://...',
    requiresCredential: true,
    allowsLocalCredential: true,
    requiresEndpoint: true,
    runtimeChat: CLOUD_DISCOVERY_ONLY_CHAT,
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
    runtimeChat: CLOUD_DISCOVERY_ONLY_CHAT,
    models: [],
  },
]

export function getModelProvider(
  provider: string,
): ModelProviderOption | undefined {
  return MODEL_PROVIDER_CATALOG.find((item) => item.id === provider)
}

export function getRuntimeChatCapability(
  provider: string,
): RuntimeChatCapability {
  return getModelProvider(provider)?.runtimeChat ?? UNKNOWN_RUNTIME_CHAT
}

export function getModelCatalogEntry(
  model: ModelRef,
): ModelCatalogEntry | undefined {
  return MODEL_PROVIDER_CATALOG.flatMap((provider) => provider.models).find(
    (entry) =>
      entry.model.provider === model.provider && entry.model.name === model.name,
  )
}
