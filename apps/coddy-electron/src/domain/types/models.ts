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

export interface ModelProviderOption {
  id: ModelProviderId
  label: string
  shortLabel: string
  description: string
  connectionLabel: string
  routingLabel: string
  connectionKind: ProviderConnectionKind
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
    models: [
      {
        model: { provider: 'ollama', name: 'gemma4-E2B' },
        label: 'gemma4-E2B',
        description: 'Perfil local padrao do Coddy.',
        tags: ['local', 'fast'],
      },
      {
        model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
        label: 'qwen2.5:0.5b',
        description: 'Resposta rapida para fluxos leves.',
        tags: ['local', 'small'],
      },
      {
        model: { provider: 'ollama', name: 'qwen2.5:7b' },
        label: 'qwen2.5:7b',
        description: 'Modelo local balanceado para codigo.',
        tags: ['local', 'code'],
      },
      {
        model: { provider: 'ollama', name: 'llama3.2:3b' },
        label: 'llama3.2:3b',
        description: 'Modelo local compacto para conversas.',
        tags: ['local', 'compact'],
      },
    ],
  },
  {
    id: 'openai',
    label: 'OpenAI',
    shortLabel: 'OpenAI',
    description: 'Modelos via API OpenAI para coding e tool use.',
    connectionLabel: 'api key',
    routingLabel: 'responses',
    connectionKind: 'api_key',
    models: [
      {
        model: { provider: 'openai', name: 'gpt-5.2' },
        label: 'GPT-5.2',
        description: 'Modelo frontier para coding agentic.',
        tags: ['code', 'tools'],
      },
      {
        model: { provider: 'openai', name: 'gpt-5-mini' },
        label: 'GPT-5 mini',
        description: 'Opcao eficiente para tarefas bem definidas.',
        tags: ['fast', 'tools'],
      },
      {
        model: { provider: 'openai', name: 'gpt-4.1' },
        label: 'GPT-4.1',
        description: 'Modelo nao-reasoning forte em tool calling.',
        tags: ['tools', 'long-context'],
      },
    ],
  },
  {
    id: 'openrouter',
    label: 'OpenRouter',
    shortLabel: 'Router',
    description: 'Roteamento OpenAI-compatible para multiplos vendors.',
    connectionLabel: 'api key',
    routingLabel: 'multi-provider',
    connectionKind: 'api_key',
    models: [
      {
        model: { provider: 'openrouter', name: 'qwen/qwen3-coder-next' },
        label: 'Qwen3 Coder Next',
        description: 'Modelo focado em coding agents e contexto longo.',
        tags: ['code', 'agentic'],
      },
      {
        model: { provider: 'openrouter', name: 'qwen/qwen3-coder' },
        label: 'Qwen3 Coder',
        description: 'Opcao de codigo para execucao via OpenRouter.',
        tags: ['code', 'router'],
      },
    ],
  },
  {
    id: 'vertex',
    label: 'Google Vertex',
    shortLabel: 'Vertex',
    description: 'Gemini via Vertex AI com identidade Google Cloud.',
    connectionLabel: 'gcloud',
    routingLabel: 'project scoped',
    connectionKind: 'workload_identity',
    models: [
      {
        model: { provider: 'vertex', name: 'gemini-2.5-pro' },
        label: 'Gemini 2.5 Pro',
        description: 'Modelo Gemini avancado para codigo e contexto longo.',
        tags: ['code', 'multimodal'],
      },
      {
        model: { provider: 'vertex', name: 'gemini-2.5-flash' },
        label: 'Gemini 2.5 Flash',
        description: 'Opcao rapida de bom custo-beneficio.',
        tags: ['fast', 'multimodal'],
      },
    ],
  },
  {
    id: 'azure',
    label: 'Azure OpenAI',
    shortLabel: 'Azure',
    description: 'Deployments Azure OpenAI gerenciados por recurso.',
    connectionLabel: 'resource',
    routingLabel: 'deployment',
    connectionKind: 'azure_resource',
    models: [
      {
        model: { provider: 'azure', name: 'gpt-5.2' },
        label: 'GPT-5.2 deployment',
        description: 'Deployment Azure para tarefas agentic coding.',
        tags: ['deployment', 'code'],
      },
      {
        model: { provider: 'azure', name: 'gpt-4.1' },
        label: 'GPT-4.1 deployment',
        description: 'Deployment Azure para tool use e contexto longo.',
        tags: ['deployment', 'tools'],
      },
    ],
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
