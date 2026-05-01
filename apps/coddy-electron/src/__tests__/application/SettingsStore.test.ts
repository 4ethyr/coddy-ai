import { describe, expect, it } from 'vitest'
import {
  DEFAULT_EVAL_HARNESS,
  DEFAULT_FLOATING_APPEARANCE,
  DEFAULT_LOCAL_MODEL_SETTINGS,
  DEFAULT_MODEL_THINKING,
  normalizeEvalHarness,
  normalizeFloatingAppearance,
  normalizeLocalModelSettings,
  normalizeModelThinking,
} from '@/application'

describe('SettingsStore', () => {
  it('normalizes floating appearance layout and glass colors', () => {
    expect(
      normalizeFloatingAppearance({
        blurPx: 50,
        transparency: 0.1,
        glassIntensity: 0.5,
        fontFamily: 'mono',
        fontSizePx: 20,
        textColor: '#101010',
        boldTextColor: '#ffffff',
        accentColor: '#00ffaa',
        glassPrimaryColor: '#112233',
        glassSecondaryColor: '#445566',
      }),
    ).toEqual({
      blurPx: 48,
      transparency: 0.32,
      glassIntensity: 0.32,
      fontFamily: 'mono',
      fontSizePx: 18,
      textColor: '#101010',
      boldTextColor: '#ffffff',
      accentColor: '#00ffaa',
      glassPrimaryColor: '#112233',
      glassSecondaryColor: '#445566',
    })
  })

  it('falls back when floating appearance values are invalid', () => {
    expect(
      normalizeFloatingAppearance({
        fontFamily: 'invalid',
        textColor: 'red',
        boldTextColor: 'white',
        accentColor: 'cyan',
        glassPrimaryColor: 'blue',
        glassSecondaryColor: 'purple',
      } as never),
    ).toEqual(DEFAULT_FLOATING_APPEARANCE)
  })

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

  it('normalizes the preferred local model provider', () => {
    expect(
      normalizeLocalModelSettings({
        providerPreference: 'vllm',
      }),
    ).toEqual({
      providerPreference: 'vllm',
    })
  })

  it('falls back when local model provider settings are invalid or absent', () => {
    expect(
      normalizeLocalModelSettings({
        providerPreference: 'bad-provider',
      } as never),
    ).toEqual(DEFAULT_LOCAL_MODEL_SETTINGS)
    expect(normalizeLocalModelSettings(undefined)).toEqual(
      DEFAULT_LOCAL_MODEL_SETTINGS,
    )
  })
})
