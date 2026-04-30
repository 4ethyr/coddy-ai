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
  SubagentLifecycleStatus,
  SubagentLifecycleUpdate,
  ReplEvent,
  ReplEventEnvelope,
  ReplSessionSnapshot,
  ReplSessionSnapshotSession,
} from './events'

export type {
  ModelCatalogEntry,
  ModelProviderId,
  ModelProviderListRequest,
  ModelProviderListResult,
  ModelProviderOption,
  ProviderConnectionKind,
  RuntimeChatCapability,
  RuntimeChatSupport,
} from './models'

export {
  MODEL_PROVIDER_CATALOG,
  getModelCatalogEntry,
  getModelProvider,
  getRuntimeChatCapability,
} from './models'

export type {
  MultiagentEvalComparison,
  MultiagentEvalRequest,
  MultiagentEvalResult,
  MultiagentEvalSuiteSummary,
  PromptBatteryFailure,
  PromptBatteryResult,
} from './evals'

export type {
  SessionStatus,
  AssessmentPolicy as SessionAssessmentPolicy,
  VoiceState,
  ContextItem,
  SubagentActivity,
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
