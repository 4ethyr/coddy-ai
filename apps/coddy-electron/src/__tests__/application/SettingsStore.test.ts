import { describe, expect, it } from 'vitest'
import {
  DEFAULT_MODEL_THINKING,
  normalizeModelThinking,
} from '@/application'

describe('SettingsStore', () => {
  it('normalizes model thinking settings with safe bounds', () => {
    expect(
      normalizeModelThinking({
        enabled: false,
        effort: 'deep',
        budgetTokens: 50_000,
        animation: 'orbit',
      }),
    ).toEqual({
      enabled: false,
      effort: 'deep',
      budgetTokens: 32_768,
      animation: 'orbit',
    })
  })

  it('falls back when model thinking settings are invalid or absent', () => {
    expect(
      normalizeModelThinking({
        effort: 'invalid',
        animation: 'invalid',
      } as never),
    ).toEqual(DEFAULT_MODEL_THINKING)
  })
})
