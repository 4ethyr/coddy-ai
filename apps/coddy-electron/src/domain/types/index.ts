// domain/types/index.ts — barrel exports

export type {
  ShortcutSource,
  ReplIntent,
  ToolStatus,
  ReplMode,
  ModelRef,
  ModelRole,
  ExtractionSource,
  ReplMessage,
  PermissionReply,
  PermissionRequest,
  ReplEvent,
  ReplEventEnvelope,
  ReplSessionSnapshot,
  ReplSessionSnapshotSession,
} from './events'

export type {
  ModelCatalogEntry,
  ModelProviderId,
  ModelProviderOption,
  ProviderConnectionKind,
} from './models'

export {
  MODEL_PROVIDER_CATALOG,
  getModelCatalogEntry,
  getModelProvider,
} from './models'

export type {
  SessionStatus,
  AssessmentPolicy as SessionAssessmentPolicy,
  VoiceState,
  ContextItem,
  ScreenUnderstandingContext,
  ReplSession,
} from './session'

export { createInitialSession } from './session'

export type {
  ApprovalPolicy,
  ReplToolCatalogItem,
  ToolCategory,
  ToolPermission,
  ToolRiskLevel,
} from './tools'

export type {
  AssessmentPolicy,
  RequestedHelp,
  ScreenAssistMode,
  AssistanceFallback,
  AssistanceDecision,
} from './policy'

export { allow, block, confirm } from './policy'
