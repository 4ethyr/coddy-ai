import type { AgentRunSummary, ModelRef } from '@/domain/types/events'

export interface AgentRunRecoveryNotice {
  title: string
  technicalCode: string
  message: string
  action: string
}

export function buildAgentRunRecoveryNotice(
  summary: AgentRunSummary,
  selectedModel?: ModelRef,
): AgentRunRecoveryNotice | null {
  if (!summary.failure_code || !summary.recoverable_failure) return null

  return {
    title: 'Recoverable model failure',
    technicalCode: summary.failure_code,
    message: redactProviderSecrets(
      summary.failure_message?.trim()
      || 'Coddy did not receive a usable model response.',
    ),
    action: recoveryAction(summary.failure_code, selectedModel?.provider),
  }
}

export function formatAgentRunRecoveryDiagnostics(
  notice: AgentRunRecoveryNotice,
): string {
  return [
    'Coddy recoverable agent failure',
    `title=${redactProviderSecrets(notice.title)}`,
    `technicalCode=${redactProviderSecrets(notice.technicalCode)}`,
    `message=${redactProviderSecrets(notice.message)}`,
    `action=${redactProviderSecrets(notice.action)}`,
  ].join('\n')
}

function recoveryAction(code: string, provider?: string): string {
  if (provider === 'openrouter') {
    return 'Retry this prompt. If it repeats, reduce context/tool output, refresh OpenRouter routing, or select another provider/model.'
  }

  if (code === 'invalid_provider_response') {
    return 'Retry this prompt. If it repeats, reduce context/tool output or select another provider/model.'
  }

  if (code === 'transport_error') {
    return 'Retry when the provider is reachable, or switch provider/model if the outage persists.'
  }

  return 'Retry this request, or switch provider/model if the failure persists.'
}

function redactProviderSecrets(text: string): string {
  return text
    .replace(/\bsk-or-[A-Za-z0-9_-]+/g, '[REDACTED]')
    .replace(/\bsk-[A-Za-z0-9_-]+/g, '[REDACTED]')
    .replace(/\bAIza[A-Za-z0-9._-]+/g, '[REDACTED]')
    .replace(/\bya29\.[A-Za-z0-9._-]+/g, '[REDACTED]')
    .replace(/\bBearer\s+[A-Za-z0-9._-]+/gi, 'Bearer [REDACTED]')
    .replace(/\b(api-key|api_key|apikey)\s*:\s*\S+/gi, '$1: [REDACTED]')
    .replace(/"((?:apiKey)|(?:api_key)|(?:token)|(?:access_token))"\s*:\s*"[^"]+"/gi, '"$1":"[REDACTED]"')
}
