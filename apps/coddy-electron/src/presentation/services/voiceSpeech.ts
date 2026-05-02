import type {
  ReplCommandResult,
  VoiceCaptureOptions,
} from '@/domain/contracts'

const MAX_SPEECH_CHARS = 4_000

type SpeechWindow = Window &
  typeof globalThis & {
    SpeechSynthesisUtterance?: typeof SpeechSynthesisUtterance
  }

export async function captureVoiceWithOptionalSpeech(
  captureVoice: (options?: VoiceCaptureOptions) => Promise<ReplCommandResult>,
  speakResponse: boolean,
): Promise<ReplCommandResult> {
  const result = await captureVoice({ speakResponse })
  if (speakResponse) {
    speakCommandResultWithBrowserFallback(result)
  }
  return result
}

export function speakCommandResultWithBrowserFallback(
  result: ReplCommandResult,
  targetWindow: SpeechWindow | undefined = defaultSpeechWindow(),
): boolean {
  const text = speechTextFromCommandResult(result)
  if (!text) return false
  return speakTextWithBrowserFallback(text, targetWindow)
}

export function speakTextWithBrowserFallback(
  text: string,
  targetWindow: SpeechWindow | undefined = defaultSpeechWindow(),
): boolean {
  const normalizedText = normalizeSpeechText(text)
  if (!normalizedText || !targetWindow) return false

  const synthesis = targetWindow.speechSynthesis
  const Utterance = targetWindow.SpeechSynthesisUtterance
  if (!synthesis || typeof Utterance !== 'function') return false

  synthesis.cancel()
  const utterance = new Utterance(normalizedText)
  utterance.rate = 1
  utterance.pitch = 1
  synthesis.speak(utterance)
  return true
}

export function cancelBrowserSpeech(
  targetWindow: SpeechWindow | undefined = defaultSpeechWindow(),
): boolean {
  if (!targetWindow?.speechSynthesis) return false
  targetWindow.speechSynthesis.cancel()
  return true
}

export function isBrowserSpeechActive(
  targetWindow: SpeechWindow | undefined = defaultSpeechWindow(),
): boolean {
  const synthesis = targetWindow?.speechSynthesis
  return Boolean(synthesis?.speaking || synthesis?.pending)
}

export function speechTextFromCommandResult(
  result: ReplCommandResult,
): string | null {
  if (result.error) return null
  return normalizeSpeechText(result.text ?? result.summary ?? result.message ?? '')
}

function normalizeSpeechText(text: string): string {
  return stripMarkdownForSpeech(text)
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, MAX_SPEECH_CHARS)
}

function stripMarkdownForSpeech(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, ' code block ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/^[#>\-*+\s]+/gm, '')
    .replace(/[*_~|]/g, ' ')
}

function defaultSpeechWindow(): SpeechWindow | undefined {
  return typeof window === 'undefined' ? undefined : (window as SpeechWindow)
}
