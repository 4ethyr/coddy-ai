import { describe, expect, it } from 'vitest'
import { redactSensitiveLogText } from '../../main/sensitiveLogRedaction'

describe('sensitiveLogRedaction', () => {
  it('redacts provider credentials from log text', () => {
    const text = [
      'CODDY_EPHEMERAL_MODEL_CREDENTIAL={"provider":"azure","token":"azure-secret","endpoint":"https://coddy-resource.openai.azure.com"}',
      'Authorization: Bearer ya29.oauth-token',
      'api-key: sk-live-secret',
      '{"apiKey":"openai-secret","token":"openrouter-secret"}',
    ].join('\n')

    const redacted = redactSensitiveLogText(text)

    expect(redacted).not.toContain('azure-secret')
    expect(redacted).not.toContain('ya29.oauth-token')
    expect(redacted).not.toContain('sk-live-secret')
    expect(redacted).not.toContain('openai-secret')
    expect(redacted).not.toContain('openrouter-secret')
    expect(redacted).toContain('"token":"[REDACTED]"')
    expect(redacted).toContain('"apiKey":"[REDACTED]"')
    expect(redacted).toContain('Bearer [REDACTED]')
    expect(redacted).toContain('api-key: [REDACTED]')
  })
})
