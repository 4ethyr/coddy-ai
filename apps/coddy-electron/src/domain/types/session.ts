// domain/types/session.ts
// Mirrors: crates/coddy-core/src/session.rs

import type {
  ReplMode,
  ReplMessage,
  ModelRef,
  PermissionRequest,
  SubagentLifecycleStatus,
  ToolStatus,
} from './events'

export type SessionStatus =
  | 'Idle'
  | 'Listening'
  | 'Transcribing'
  | 'CapturingScreen'
  | 'BuildingContext'
  | 'Thinking'
  | 'Streaming'
  | 'Speaking'
  | 'AwaitingConfirmation'
  | 'AwaitingToolApproval'
  | 'Error'

export type AssessmentPolicy =
  | 'Practice'
  | 'PermittedAi'
  | 'SyntaxOnly'
  | 'RestrictedAssessment'
  | 'UnknownAssessment'

export interface VoiceState {
  enabled: boolean
  speaking: boolean
  muted: boolean
}

export interface ContextItem {
  id: string
  label: string
  sensitive: boolean
}

export type ToolActivityStatus = 'Running' | ToolStatus

export interface ToolActivity {
  id: string
  name: string
  status: ToolActivityStatus
}

export interface SubagentActivity {
  id: string
  name: string
  mode: string
  status: SubagentLifecycleStatus
  readiness_score: number
  required_output_fields: string[]
  output_additional_properties_allowed: boolean
  reason: string | null
}

export interface ScreenUnderstandingContext {
  source_app: string | null
  visible_text: string
  detected_kind: string
  confidence: number
}

export interface ReplSession {
  id: string
  mode: ReplMode
  status: SessionStatus
  policy: AssessmentPolicy
  selected_model: ModelRef
  voice: VoiceState
  screen_context: ScreenUnderstandingContext | null
  workspace_context: ContextItem[]
  messages: ReplMessage[]
  active_run: string | null
  pending_permission: PermissionRequest | null
  tool_activity: ToolActivity[]
  subagent_activity: SubagentActivity[]
  /** Frontend-only: text being accumulated from TokenDelta events */
  streaming_text: string
}

export function createInitialSession(mode: ReplMode, model: ModelRef): ReplSession {
  return {
    id: '',
    mode,
    status: 'Idle',
    policy: 'UnknownAssessment',
    selected_model: model,
    voice: { enabled: true, speaking: false, muted: false },
    screen_context: null,
    workspace_context: [],
    messages: [],
    active_run: null,
    pending_permission: null,
    tool_activity: [],
    subagent_activity: [],
    streaming_text: '',
  }
}
