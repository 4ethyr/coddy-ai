// domain/types/events.ts
// Mirrors: crates/coddy-core/src/event.rs

import type { ToolPermission, ToolRiskLevel } from './tools'
import type { ContextItem } from './session'

export type ShortcutSource = 'GnomeMediaKeys' | 'TauriGlobalShortcut' | 'Cli' | 'SystemdUserService'

export type ReplIntent =
  | 'AskTechnicalQuestion'
  | 'ExplainScreen'
  | 'ExplainCode'
  | 'DebugCode'
  | 'SolvePracticeQuestion'
  | 'MultipleChoiceAssist'
  | 'GenerateTestCases'
  | 'ExplainTerminalError'
  | 'SearchDocs'
  | 'OpenApplication'
  | 'OpenWebsite'
  | 'ConfigureModel'
  | 'ManageContext'
  | 'AgenticCodeChange'
  | 'Unknown'

export type ToolStatus = 'Succeeded' | 'Failed' | 'Cancelled' | 'Denied'

export interface SubagentRouteRecommendation {
  name: string
  score: number
  mode: string
  matched_signals: string[]
}

export type ReplMode = 'FloatingTerminal' | 'DesktopApp'

export type ModelRole = 'Chat' | 'Ocr' | 'Asr' | 'Tts' | 'Embedding'

export interface ModelRef {
  provider: string
  name: string
}

export type ExtractionSource = 'Accessibility' | 'BrowserDom' | 'ScreenshotOcr' | 'UserProvidedText'

export interface ReplMessage {
  id: string
  role: string
  text: string
}

export type PermissionReply = 'Once' | 'Always' | 'Reject'

export interface PermissionRequest {
  id: string
  session_id: string
  run_id: string
  tool_call_id: string | null
  tool_name: string
  permission: ToolPermission
  patterns: string[]
  risk_level: ToolRiskLevel
  metadata: unknown
  requested_at_unix_ms: number
}

// Mirrors coddy-core ReplEvent enum
export type ReplEvent =
  | { SessionStarted: { session_id: string } }
  | { RunStarted: { run_id: string } }
  | { ShortcutTriggered: { binding: string; source: ShortcutSource } }
  | { OverlayShown: { mode: ReplMode } }
  | { VoiceListeningStarted: Record<string, never> }
  | { VoiceTranscriptPartial: { text: string } }
  | { VoiceTranscriptFinal: { text: string } }
  | { ScreenCaptured: { source: ExtractionSource; bytes: number } }
  | { OcrCompleted: { chars: number; language_hint?: string } }
  | { IntentDetected: { intent: ReplIntent; confidence: number } }
  | { PolicyEvaluated: { policy: string; allowed: boolean } }
  | { ConfirmationDismissed: Record<string, never> }
  | { ModelSelected: { model: ModelRef; role: ModelRole } }
  | { SearchStarted: { query: string; provider: string } }
  | { SearchContextExtracted: { provider: string; organic_results: number; ai_overview_present: boolean } }
  | { ContextItemAdded: { item: ContextItem } }
  | { TokenDelta: { run_id: string; text: string } }
  | { MessageAppended: { message: ReplMessage } }
  | { ToolStarted: { name: string } }
  | { ToolCompleted: { name: string; status: ToolStatus } }
  | { SubagentRouted: { recommendations: SubagentRouteRecommendation[] } }
  | { PermissionRequested: { request: PermissionRequest } }
  | { PermissionReplied: { request_id: string; reply: PermissionReply } }
  | { TtsQueued: Record<string, never> }
  | { TtsStarted: Record<string, never> }
  | { TtsCompleted: Record<string, never> }
  | { RunCompleted: { run_id: string } }
  | { Error: { code: string; message: string } }

export interface ReplEventEnvelope {
  sequence: number
  session_id: string
  run_id: string | null
  captured_at_unix_ms: number
  event: ReplEvent
}

/** Mirrors crates/coddy-core/src/event_log.rs */
export interface ReplSessionSnapshot {
  /** Raw session from the daemon — fields use string enums from JSON */
  session: ReplSessionSnapshotSession
  last_sequence: number
}

/** JSON-serialized session (enums are strings at the wire level) */
export interface ReplSessionSnapshotSession {
  id: string
  mode: ReplMode
  status: string
  policy: string
  selected_model: { provider: string; name: string }
  voice: { enabled: boolean; speaking: boolean; muted: boolean }
  screen_context: unknown
  workspace_context: ContextItem[]
  messages: ReplMessage[]
  active_run: string | null
  pending_permission?: PermissionRequest | null
  tool_activity?: unknown[]
  streaming_text?: string
}
