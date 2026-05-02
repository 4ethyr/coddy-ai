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
  openConversation,
  startNewSession,
  selectModel,
  listProviderModels,
  runMultiagentEval,
  runPromptBatteryEval,
  getActiveWorkspace,
  loadConversationHistory,
  selectWorkspaceFolder,
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
  DEFAULT_LOCAL_MODEL_SETTINGS,
  DEFAULT_MODEL_THINKING,
  loadSettings,
  normalizeEvalHarness,
  normalizeFloatingAppearance,
  normalizeLocalModelSettings,
  normalizeModelThinking,
  saveSettings,
} from './SettingsStore'
export type {
  EvalHarnessSettings,
  FloatingFontFamily,
  FloatingAppearanceSettings,
  LocalModelProviderPreference,
  LocalModelSettings,
  ModelThinkingEffort,
  ModelThinkingSettings,
  ThinkingAnimation,
  UserSettings,
} from './SettingsStore'
