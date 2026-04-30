import { describe, expect, it } from 'vitest'
import { getRuntimeTtsCapability } from '@/domain'

describe('model capabilities', () => {
  it('routes explicit audio or TTS models to native TTS', () => {
    expect(
      getRuntimeTtsCapability({
        provider: 'openai',
        name: 'gpt-4o-mini-tts',
      }).route,
    ).toBe('native')

    expect(
      getRuntimeTtsCapability(
        { provider: 'vertex', name: 'gemini-flash' },
        ['text', 'speech'],
      ).route,
    ).toBe('native')
  })

  it('falls back for plain chat models without speech capability signals', () => {
    expect(
      getRuntimeTtsCapability({
        provider: 'vertex',
        name: 'gemini-3.1-flash-lite-preview',
      }).route,
    ).toBe('fallback')
  })
})
