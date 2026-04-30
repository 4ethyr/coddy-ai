// application/index.ts

export { initializeSession, createLocalSession, applyEvents } from './SessionManager'
export type { SessionState } from './SessionManager'

export { startEventStream } from './EventStreamer'
export type { StreamCallback, ErrorCallback } from './EventStreamer'

export {
  sendAsk,
  sendVoiceTurn,
  cancelRun,
  cancelSpeech,
  selectModel,
  listProviderModels,
  runMultiagentEval,
  runPromptBatteryEval,
  openUi,
  captureVoice,
  cancelVoiceCapture,
  captureAndExplain,
  dismissConfirmation,
  replyPermission,
  ReplCommandError,
} from './CommandSender'

export {
  DEFAULT_FLOATING_APPEARANCE,
  DEFAULT_EVAL_HARNESS,
  DEFAULT_MODEL_THINKING,
  loadSettings,
  normalizeEvalHarness,
  normalizeFloatingAppearance,
  normalizeModelThinking,
  saveSettings,
} from './SettingsStore'
export type {
  EvalHarnessSettings,
  FloatingFontFamily,
  FloatingAppearanceSettings,
  ModelThinkingEffort,
  ModelThinkingSettings,
  ThinkingAnimation,
  UserSettings,
} from './SettingsStore'
