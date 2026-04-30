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
  DEFAULT_MODEL_THINKING,
  loadSettings,
  normalizeFloatingAppearance,
  normalizeModelThinking,
  saveSettings,
} from './SettingsStore'
export type {
  FloatingAppearanceSettings,
  ModelThinkingEffort,
  ModelThinkingSettings,
  ThinkingAnimation,
  UserSettings,
} from './SettingsStore'
