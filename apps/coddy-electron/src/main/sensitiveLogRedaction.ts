const REDACTED = '[REDACTED]'

export function redactSensitiveLogText(text: string): string {
  return text
    .replace(
      /("?(?:apiKey|token)"?\s*:\s*")([^"]+)(")/gi,
      `$1${REDACTED}$3`,
    )
    .replace(/\bBearer\s+[A-Za-z0-9._-]+/g, `Bearer ${REDACTED}`)
    .replace(/\bya29\.[A-Za-z0-9._-]+/g, `ya29.${REDACTED}`)
    .replace(/\bsk-[A-Za-z0-9_-]+/g, `sk-${REDACTED}`)
    .replace(/\bapi-key(\s*[:=]\s*)[^\s"',}]+/gi, `api-key$1${REDACTED}`)
}
