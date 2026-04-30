import { describe, expect, it } from 'vitest'
import {
  DEFAULT_EVAL_HARNESS,
  DEFAULT_MODEL_THINKING,
  normalizeEvalHarness,
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

  it('normalizes eval harness paths before persistence', () => {
    expect(
      normalizeEvalHarness({
        baselinePath: ' /tmp/baseline.json ',
        writeBaselinePath: ' /tmp/latest.json ',
      }),
    ).toEqual({
      baselinePath: '/tmp/baseline.json',
      writeBaselinePath: '/tmp/latest.json',
    })
  })

  it('falls back when eval harness paths are absent', () => {
    expect(normalizeEvalHarness(undefined)).toEqual(DEFAULT_EVAL_HARNESS)
  })
})
