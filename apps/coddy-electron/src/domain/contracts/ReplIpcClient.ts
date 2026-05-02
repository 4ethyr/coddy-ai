// domain/contracts/ReplIpcClient.ts
// Port interface — implemented by infrastructure

import type {
  ModelRef,
  ModelProviderListRequest,
  ModelProviderListResult,
  ModelRole,
  ModelSelectionOptions,
  MultiagentEvalRequest,
  MultiagentEvalResult,
  PromptBatteryResult,
  QualityEvalResult,
  PermissionReply,
  ReplEventEnvelope,
  ReplMode,
  ReplSessionSnapshot,
  ScreenAssistMode,
  AssessmentPolicy,
  ConversationRecord,
  ReplToolCatalogItem,
} from '../types'

/** Result of sending a command to the REPL backend */
export interface ReplCommandResult {
  text?: string
  summary?: string
  message?: string
  error?: { code: string; message: string }
}

export interface WorkspaceSelectionResult {
  path: string | null
  cancelled?: boolean
  message?: string
  error?: { code: string; message: string }
}

export interface VoiceCaptureOptions {
  speakResponse?: boolean
}

/** Batch of incremental events */
export interface ReplEventsBatch {
  events: ReplEventEnvelope[]
  lastSequence: number
}

/**
 * Abstraction over the Coddy REPL backend transport.
 *
 * The frontend never knows whether it's talking to:
 * - a spawned `coddy` CLI child process
 * - a Unix socket via Tauri command
 * - an HTTP bridge
 *
 * It just calls these methods and gets typed results.
 */
export interface ReplIpcClient {
  /** Get a full session snapshot (state + last sequence) */
  getSnapshot(): Promise<ReplSessionSnapshot>

  /** Get incremental events after a given sequence number */
  getEventsAfter(afterSequence: number): Promise<ReplEventsBatch>

  /** Get the backend tool catalog exposed by the active Coddy runtime */
  getToolCatalog(): Promise<ReplToolCatalogItem[]>

  /** Get persisted redacted chat history from the active runtime */
  getConversationHistory(limit?: number): Promise<ConversationRecord[]>

  /** Get the Electron-selected filesystem workspace, if one is active */
  getActiveWorkspace(): Promise<WorkspaceSelectionResult>

  /** Let the user select a local folder and restart Coddy against that workspace */
  selectWorkspaceFolder(): Promise<WorkspaceSelectionResult>

  /** Run the deterministic multiagent harness and optionally compare/write a baseline */
  runMultiagentEval(
    request?: MultiagentEvalRequest,
  ): Promise<MultiagentEvalResult>

  /** Run the deterministic 1,200-prompt routing battery without model API spend */
  runPromptBatteryEval(): Promise<PromptBatteryResult>

  /** Run the combined deterministic quality gate without model API spend */
  runQualityEval(): Promise<QualityEvalResult>

  /** List available models for a provider using a session-scoped credential */
  listProviderModels(
    request: ModelProviderListRequest,
  ): Promise<ModelProviderListResult>

  /** Open a persistent stream of live events. Returns an AsyncIterable. */
  watchEvents(afterSequence: number): AsyncIterable<ReplEventEnvelope>

  /** Send an `ask` text command */
  ask(text: string): Promise<ReplCommandResult>

  /** Send a `voice turn` command (with pre-transcribed text) */
  voiceTurn(transcript: string): Promise<ReplCommandResult>

  /** Stop the active assistant run (cancel generation) */
  stopActiveRun(): Promise<void>

  /** Archive the current conversation and start a new daemon session */
  newSession(): Promise<ReplCommandResult>

  /** Restore a persisted conversation as the active daemon session */
  openConversation(sessionId: string): Promise<ReplCommandResult>

  /** Stop TTS speech immediately */
  stopSpeaking(): Promise<void>

  /** Select a model for a specific REPL role */
  selectModel(
    model: ModelRef,
    role: ModelRole,
    options?: ModelSelectionOptions,
  ): Promise<ReplCommandResult>

  /** Ask the backend to open/switch the REPL UI mode */
  openUi(mode: ReplMode): Promise<ReplCommandResult>

  /** Request a policy-aware screen assist run */
  captureAndExplain(
    mode: ScreenAssistMode,
    policy: AssessmentPolicy,
  ): Promise<ReplCommandResult>

  /** Dismiss a pending policy confirmation without sending prompt text */
  dismissConfirmation(): Promise<ReplCommandResult>

  /** Reply to a pending backend tool permission request */
  replyPermission(
    requestId: string,
    reply: PermissionReply,
  ): Promise<ReplCommandResult>

  /**
   * Capture voice via the system mic (spawns `coddy voice --overlay`).
   * The CLI handles recording, STT, and sends VoiceTurn to the daemon.
   * Returns the text result or an error.
   */
  captureVoice(options?: VoiceCaptureOptions): Promise<ReplCommandResult>

  /** Cancel the active microphone capture, if one is running */
  cancelVoiceCapture(): Promise<void>
}
