import type { ModelRef } from './events'

export type ModelProviderId =
  | 'ollama'
  | 'openai'
  | 'openrouter'
  | 'nvidia'
  | 'vertex'
  | 'azure'

export type ProviderConnectionKind =
  | 'local'
  | 'api_key'
  | 'workload_identity'
  | 'azure_resource'

export type RuntimeChatSupport = 'supported' | 'adapter_pending'
export type RuntimeTtsRoute = 'native' | 'fallback_required'
export type LocalModelProviderPreference = 'auto' | 'ollama' | 'hf' | 'vllm'

export interface RuntimeChatCapability {
  status: RuntimeChatSupport
  label: string
  description: string
}

export interface RuntimeTtsCapability {
  route: RuntimeTtsRoute
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
  apiVersion?: string
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

export interface ModelSelectionOptions {
  localProviderPreference?: LocalModelProviderPreference
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
  apiVersionLabel?: string
  apiVersionPlaceholder?: string
  requiresCredential: boolean
  allowsLocalCredential?: boolean
  requiresEndpoint?: boolean
  supportsApiVersion?: boolean
  runtimeChat: RuntimeChatCapability
  models: readonly ModelCatalogEntry[]
}

const LOCAL_RUNTIME_CHAT: RuntimeChatCapability = {
  status: 'supported',
  label: 'runtime ready',
  description:
    'Chat execution, streaming events and safe low-risk tools are wired through the Rust runtime.',
}

const OPENAI_COMPATIBLE_RUNTIME_CHAT: RuntimeChatCapability = {
  status: 'supported',
  label: 'runtime ready',
  description:
    'Chat execution uses the Rust OpenAI-compatible adapter with request-scoped credentials from the secure main-process bridge.',
}

const VERTEX_RUNTIME_CHAT: RuntimeChatCapability = {
  status: 'supported',
  label: 'runtime ready',
  description:
    'Gemini API-key models execute through generateContent, and Claude partner models execute through Vertex AI rawPredict with OAuth/ADC/gcloud project metadata.',
}

const AZURE_OPENAI_RUNTIME_CHAT: RuntimeChatCapability = {
  status: 'supported',
  label: 'runtime ready',
  description:
    'Azure deployments execute through the Rust Azure OpenAI chat completions adapter with endpoint-scoped API keys.',
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
    runtimeChat: OPENAI_COMPATIBLE_RUNTIME_CHAT,
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
    runtimeChat: OPENAI_COMPATIBLE_RUNTIME_CHAT,
    models: [],
  },
  {
    id: 'nvidia',
    label: 'NVIDIA NIM',
    shortLabel: 'NVIDIA',
    description: 'Modelos NVIDIA NIM via API OpenAI-compatible.',
    connectionLabel: 'api key',
    routingLabel: 'nim',
    connectionKind: 'api_key',
    credentialLabel: 'NVIDIA API key',
    credentialPlaceholder: 'nvapi-...',
    requiresCredential: true,
    runtimeChat: OPENAI_COMPATIBLE_RUNTIME_CHAT,
    models: [
      {
        model: { provider: 'nvidia', name: 'deepseek-ai/deepseek-v4-pro' },
        label: 'DeepSeek V4 Pro',
        description: 'DeepSeek V4 Pro served through NVIDIA NIM.',
        tags: ['api', 'coding'],
      },
    ],
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
    runtimeChat: VERTEX_RUNTIME_CHAT,
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
    apiVersionLabel: 'Azure API version',
    apiVersionPlaceholder: '2024-10-21',
    requiresCredential: true,
    requiresEndpoint: true,
    supportsApiVersion: true,
    runtimeChat: AZURE_OPENAI_RUNTIME_CHAT,
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

export function getRuntimeTtsCapability(
  model: Pick<ModelRef, 'provider' | 'name'>,
  tags: readonly string[] = [],
): RuntimeTtsCapability {
  const tokens = [model.provider, model.name, ...tags]
    .join(' ')
    .toLowerCase()
  const hasNativeTtsSignal =
    /\b(tts|speech|audio|realtime|voice)\b/.test(tokens)
    || /(^|[-_/])tts([-_/]|$)/.test(tokens)
    || /(^|[-_/])audio([-_/]|$)/.test(tokens)

  if (hasNativeTtsSignal) {
    return {
      route: 'native',
      label: 'native TTS',
      description:
        'The selected model advertises audio, speech or TTS capability, so Coddy can route speech through the model provider.',
    }
  }

  return {
    route: 'fallback_required',
    label: 'TTS fallback required',
    description:
      'The selected chat model does not advertise native TTS and will not be used for speech synthesis. Spoken responses require a configured TTS fallback.',
  }
}

export function getModelCatalogEntry(
  model: ModelRef,
): ModelCatalogEntry | undefined {
  return MODEL_PROVIDER_CATALOG.flatMap((provider) => provider.models).find(
    (entry) =>
      entry.model.provider === model.provider && entry.model.name === model.name,
  )
}
