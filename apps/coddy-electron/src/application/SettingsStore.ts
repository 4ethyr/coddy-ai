// application/SettingsStore.ts
// Simple JSON persistence for user preferences (model, voice, theme, mode).
// Uses localStorage. Falls back to in-memory default when not in browser.

import type { LocalModelProviderPreference, ModelRef, ReplMode } from '@/domain'

export type { LocalModelProviderPreference } from '@/domain'

export interface FloatingAppearanceSettings {
  blurPx: number
  transparency: number
  glassIntensity: number
  fontFamily: FloatingFontFamily
  fontSizePx: number
  textColor: string
  boldTextColor: string
  accentColor: string
  glassPrimaryColor: string
  glassSecondaryColor: string
}

export type FloatingFontFamily = 'system' | 'mono' | 'serif' | 'display'
export type ModelThinkingEffort = 'minimal' | 'balanced' | 'deep'
export type ThinkingAnimation = 'pulse' | 'scan' | 'orbit'

export interface ModelThinkingSettings {
  enabled: boolean
  effort: ModelThinkingEffort
  budgetTokens: number
  animation: ThinkingAnimation
}

export interface EvalHarnessSettings {
  baselinePath: string
  writeBaselinePath: string
}

export interface LocalModelSettings {
  providerPreference: LocalModelProviderPreference
}

export interface UserSettings {
  selectedModel: ModelRef
  mode: ReplMode
  voiceEnabled: boolean
  voiceMuted: boolean
  floatingAppearance: FloatingAppearanceSettings
  modelThinking: ModelThinkingSettings
  evalHarness: EvalHarnessSettings
  localModel: LocalModelSettings
}

const STORAGE_KEY = 'coddy:settings'

export const DEFAULT_FLOATING_APPEARANCE: FloatingAppearanceSettings = {
  blurPx: 24,
  transparency: 0.58,
  glassIntensity: 0.14,
  fontFamily: 'system',
  fontSizePx: 14,
  textColor: '#e5e2e1',
  boldTextColor: '#ffffff',
  accentColor: '#00dbe9',
  glassPrimaryColor: '#00dbe9',
  glassSecondaryColor: '#b600f8',
}

export const DEFAULT_MODEL_THINKING: ModelThinkingSettings = {
  enabled: true,
  effort: 'balanced',
  budgetTokens: 2048,
  animation: 'scan',
}

export const DEFAULT_EVAL_HARNESS: EvalHarnessSettings = {
  baselinePath: '',
  writeBaselinePath: '',
}

export const DEFAULT_LOCAL_MODEL_SETTINGS: LocalModelSettings = {
  providerPreference: 'auto',
}

const DEFAULT_SETTINGS: UserSettings = {
  selectedModel: { provider: 'ollama', name: 'gemma4:e2b' },
  mode: 'FloatingTerminal',
  voiceEnabled: true,
  voiceMuted: false,
  floatingAppearance: { ...DEFAULT_FLOATING_APPEARANCE },
  modelThinking: { ...DEFAULT_MODEL_THINKING },
  evalHarness: { ...DEFAULT_EVAL_HARNESS },
  localModel: { ...DEFAULT_LOCAL_MODEL_SETTINGS },
}

function isBrowser(): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined'
}

export function loadSettings(): UserSettings {
  if (!isBrowser()) return { ...DEFAULT_SETTINGS }

  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_SETTINGS }

    const parsed = JSON.parse(raw) as Partial<UserSettings>
    return {
      selectedModel: parsed.selectedModel ?? DEFAULT_SETTINGS.selectedModel,
      mode: parsed.mode ?? DEFAULT_SETTINGS.mode,
      voiceEnabled: parsed.voiceEnabled ?? DEFAULT_SETTINGS.voiceEnabled,
      voiceMuted: parsed.voiceMuted ?? DEFAULT_SETTINGS.voiceMuted,
      floatingAppearance: normalizeFloatingAppearance(parsed.floatingAppearance),
      modelThinking: normalizeModelThinking(parsed.modelThinking),
      evalHarness: normalizeEvalHarness(parsed.evalHarness),
      localModel: normalizeLocalModelSettings(parsed.localModel),
    }
  } catch {
    return { ...DEFAULT_SETTINGS }
  }
}

export function normalizeEvalHarness(
  value: Partial<EvalHarnessSettings> | undefined,
): EvalHarnessSettings {
  return {
    baselinePath: normalizePathDraft(
      value?.baselinePath,
      DEFAULT_EVAL_HARNESS.baselinePath,
    ),
    writeBaselinePath: normalizePathDraft(
      value?.writeBaselinePath,
      DEFAULT_EVAL_HARNESS.writeBaselinePath,
    ),
  }
}

export function normalizeModelThinking(
  value: Partial<ModelThinkingSettings> | undefined,
): ModelThinkingSettings {
  return {
    enabled:
      typeof value?.enabled === 'boolean'
        ? value.enabled
        : DEFAULT_MODEL_THINKING.enabled,
    effort: validThinkingEffort(value?.effort),
    budgetTokens: clampNumber(
      value?.budgetTokens,
      0,
      32_768,
      DEFAULT_MODEL_THINKING.budgetTokens,
    ),
    animation: validThinkingAnimation(value?.animation),
  }
}

export function normalizeLocalModelSettings(
  value: Partial<LocalModelSettings> | undefined,
): LocalModelSettings {
  return {
    providerPreference: validLocalModelProviderPreference(
      value?.providerPreference,
    ),
  }
}

export function saveSettings(settings: Partial<UserSettings>): void {
  if (!isBrowser()) return

  try {
    const current = loadSettings()
    const merged = { ...current, ...settings }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(merged))
  } catch {
    // Storage full or unavailable — silently fail
  }
}

export function normalizeFloatingAppearance(
  value: Partial<FloatingAppearanceSettings> | undefined,
): FloatingAppearanceSettings {
  return {
    blurPx: clampNumber(value?.blurPx, 0, 48, DEFAULT_FLOATING_APPEARANCE.blurPx),
    transparency: clampNumber(
      value?.transparency,
      0.32,
      0.92,
      DEFAULT_FLOATING_APPEARANCE.transparency,
    ),
    glassIntensity: clampNumber(
      value?.glassIntensity,
      0,
      0.32,
      DEFAULT_FLOATING_APPEARANCE.glassIntensity,
    ),
    fontFamily: validFloatingFontFamily(value?.fontFamily),
    fontSizePx: clampNumber(
      value?.fontSizePx,
      12,
      18,
      DEFAULT_FLOATING_APPEARANCE.fontSizePx,
    ),
    textColor: validHexColor(value?.textColor, DEFAULT_FLOATING_APPEARANCE.textColor),
    boldTextColor: validHexColor(
      value?.boldTextColor,
      DEFAULT_FLOATING_APPEARANCE.boldTextColor,
    ),
    accentColor: validHexColor(value?.accentColor, DEFAULT_FLOATING_APPEARANCE.accentColor),
    glassPrimaryColor: validHexColor(
      value?.glassPrimaryColor,
      DEFAULT_FLOATING_APPEARANCE.glassPrimaryColor,
    ),
    glassSecondaryColor: validHexColor(
      value?.glassSecondaryColor,
      DEFAULT_FLOATING_APPEARANCE.glassSecondaryColor,
    ),
  }
}

function clampNumber(
  value: number | undefined,
  min: number,
  max: number,
  fallback: number,
): number {
  if (typeof value !== 'number' || Number.isNaN(value)) return fallback
  return Math.min(max, Math.max(min, value))
}

function validHexColor(value: string | undefined, fallback: string): string {
  if (!value) return fallback
  return /^#[0-9a-f]{6}$/i.test(value) ? value : fallback
}

function validFloatingFontFamily(
  value: FloatingFontFamily | undefined,
): FloatingFontFamily {
  return value === 'system'
    || value === 'mono'
    || value === 'serif'
    || value === 'display'
    ? value
    : DEFAULT_FLOATING_APPEARANCE.fontFamily
}

function normalizePathDraft(value: string | undefined, fallback: string): string {
  if (typeof value !== 'string') return fallback
  return value.trim().slice(0, 512)
}

function validThinkingEffort(
  value: ModelThinkingEffort | undefined,
): ModelThinkingEffort {
  return value === 'minimal' || value === 'balanced' || value === 'deep'
    ? value
    : DEFAULT_MODEL_THINKING.effort
}

function validThinkingAnimation(
  value: ThinkingAnimation | undefined,
): ThinkingAnimation {
  return value === 'pulse' || value === 'scan' || value === 'orbit'
    ? value
    : DEFAULT_MODEL_THINKING.animation
}

function validLocalModelProviderPreference(
  value: LocalModelProviderPreference | undefined,
): LocalModelProviderPreference {
  return value === 'auto' || value === 'ollama' || value === 'hf' || value === 'vllm'
    ? value
    : DEFAULT_LOCAL_MODEL_SETTINGS.providerPreference
}
