import { describe, expect, it, vi } from 'vitest'
import {
  cancelBrowserSpeech,
  captureVoiceWithOptionalSpeech,
  isBrowserSpeechActive,
  speakCommandResultWithBrowserFallback,
  speechTextFromCommandResult,
} from '@/presentation/services/voiceSpeech'

describe('voice speech fallback', () => {
  it('speaks successful command text through browser speech synthesis', () => {
    const targetWindow = createSpeechWindow()

    const spoken = speakCommandResultWithBrowserFallback(
      { text: '### Result\n\nUse `cargo test`.' },
      targetWindow,
    )

    expect(spoken).toBe(true)
    expect(targetWindow.speechSynthesis.cancel).toHaveBeenCalledOnce()
    expect(targetWindow.speechSynthesis.speak).toHaveBeenCalledOnce()
    expect(targetWindow.lastUtterance?.text).toBe('Result Use cargo test.')
  })

  it('does not speak command errors', () => {
    const targetWindow = createSpeechWindow()

    const spoken = speakCommandResultWithBrowserFallback(
      { error: { code: 'VOICE_CAPTURE_FAILED', message: 'mic busy' } },
      targetWindow,
    )

    expect(spoken).toBe(false)
    expect(targetWindow.speechSynthesis.speak).not.toHaveBeenCalled()
  })

  it('captures voice and speaks only when the preference is enabled', async () => {
    const targetWindow = createSpeechWindow()
    Object.defineProperty(window, 'speechSynthesis', {
      configurable: true,
      value: targetWindow.speechSynthesis,
    })
    Object.defineProperty(window, 'SpeechSynthesisUtterance', {
      configurable: true,
      value: targetWindow.SpeechSynthesisUtterance,
    })
    const captureVoice = vi.fn().mockResolvedValue({ text: 'voice answer' })

    await captureVoiceWithOptionalSpeech(captureVoice, true)

    expect(captureVoice).toHaveBeenCalledWith({ speakResponse: true })
    expect(targetWindow.speechSynthesis.speak).toHaveBeenCalledOnce()
  })

  it('reports and cancels active browser speech', () => {
    const targetWindow = createSpeechWindow({ speaking: true })

    expect(isBrowserSpeechActive(targetWindow)).toBe(true)
    expect(cancelBrowserSpeech(targetWindow)).toBe(true)

    expect(targetWindow.speechSynthesis.cancel).toHaveBeenCalledOnce()
  })

  it('normalizes markdown text before speech', () => {
    expect(
      speechTextFromCommandResult({
        text: '| A | B |\n|---|---|\n| **ok** | [docs](https://x.test) |',
      }),
    ).toBe('A B --- --- ok docs')
  })
})

function createSpeechWindow(options: { speaking?: boolean } = {}) {
  const targetWindow = {
    lastUtterance: null as MockUtterance | null,
    SpeechSynthesisUtterance: MockUtterance,
    speechSynthesis: {
      speaking: options.speaking ?? false,
      pending: false,
      cancel: vi.fn(),
      speak: vi.fn((utterance: MockUtterance) => {
        targetWindow.lastUtterance = utterance
        targetWindow.speechSynthesis.speaking = true
      }),
    },
  }
  return targetWindow as unknown as Window &
    typeof globalThis & {
      lastUtterance: MockUtterance | null
      SpeechSynthesisUtterance: typeof MockUtterance
    }
}

class MockUtterance {
  pitch = 0
  rate = 0

  constructor(public text: string) {}
}
