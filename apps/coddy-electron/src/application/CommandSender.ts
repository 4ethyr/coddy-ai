// application/CommandSender.ts
// Use case: sends user commands to the REPL backend.

import type {
  ModelRef,
  ModelProviderListRequest,
  ModelProviderListResult,
  ModelRole,
  ModelSelectionOptions,
  MultiagentEvalRequest,
  MultiagentEvalResult,
  PromptBatteryResult,
  PermissionReply,
  ConversationRecord,
  AssessmentPolicy,
  ReplIpcClient,
  ReplCommandResult,
  ReplMode,
  ScreenAssistMode,
  WorkspaceSelectionResult,
} from '@/domain'

export class ReplCommandError extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(message)
    this.name = 'ReplCommandError'
    this.code = code
  }
}

/**
 * Sends a text question to the REPL backend.
 * The result (text/summary/error) comes back after the daemon finishes.
 */
export async function sendAsk(
  client: ReplIpcClient,
  text: string,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.ask(text))
}

/**
 * Sends a pre-transcribed voice turn.
 */
export async function sendVoiceTurn(
  client: ReplIpcClient,
  transcript: string,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.voiceTurn(transcript))
}

/**
 * Requests the daemon to stop its current generation run.
 */
export async function cancelRun(client: ReplIpcClient): Promise<void> {
  await client.stopActiveRun()
}

/**
 * Archives the current session and starts a fresh REPL conversation.
 */
export async function startNewSession(
  client: ReplIpcClient,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.newSession())
}

/**
 * Requests the daemon to stop TTS playback immediately.
 */
export async function cancelSpeech(client: ReplIpcClient): Promise<void> {
  await client.stopSpeaking()
}

/**
 * Selects a backend model for the requested REPL role.
 */
export async function selectModel(
  client: ReplIpcClient,
  model: ModelRef,
  role: ModelRole,
  options?: ModelSelectionOptions,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.selectModel(model, role, options))
}

/**
 * Lists provider models using a credential supplied for this request only.
 */
export async function listProviderModels(
  client: ReplIpcClient,
  request: ModelProviderListRequest,
): Promise<ModelProviderListResult> {
  return client.listProviderModels(request)
}

/**
 * Runs the deterministic multiagent harness exposed by the backend.
 */
export async function runMultiagentEval(
  client: ReplIpcClient,
  request: MultiagentEvalRequest = {},
): Promise<MultiagentEvalResult> {
  return client.runMultiagentEval(request)
}

/**
 * Runs the deterministic 1,200-prompt routing battery exposed by the backend.
 */
export async function runPromptBatteryEval(
  client: ReplIpcClient,
): Promise<PromptBatteryResult> {
  return client.runPromptBatteryEval()
}

export async function getActiveWorkspace(
  client: ReplIpcClient,
): Promise<WorkspaceSelectionResult> {
  return client.getActiveWorkspace()
}

export async function loadConversationHistory(
  client: ReplIpcClient,
  limit = 25,
): Promise<ConversationRecord[]> {
  return client.getConversationHistory(limit)
}

export async function selectWorkspaceFolder(
  client: ReplIpcClient,
): Promise<WorkspaceSelectionResult> {
  const result = await client.selectWorkspaceFolder()
  if (result.error) {
    throw new ReplCommandError(result.error.code, result.error.message)
  }
  return result
}

/**
 * Switches the backend REPL UI mode. The reducer applies the emitted
 * OverlayShown event so all windows converge on the daemon state.
 */
export async function openUi(
  client: ReplIpcClient,
  mode: ReplMode,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.openUi(mode))
}

/**
 * Captures voice through the platform-specific backend. In Electron this
 * already sends the transcribed VoiceTurn to the daemon, so callers must not
 * feed the returned text back into ask().
 */
export async function captureVoice(
  client: ReplIpcClient,
): Promise<ReplCommandResult> {
  return client.captureVoice()
}

/**
 * Cancels the current microphone capture process, if one exists.
 */
export async function cancelVoiceCapture(
  client: ReplIpcClient,
): Promise<void> {
  await client.cancelVoiceCapture()
}

/**
 * Requests a policy-aware screen assistance flow.
 */
export async function captureAndExplain(
  client: ReplIpcClient,
  mode: ScreenAssistMode,
  policy: AssessmentPolicy,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.captureAndExplain(mode, policy))
}

/**
 * Dismisses a pending policy confirmation without routing text to the LLM.
 */
export async function dismissConfirmation(
  client: ReplIpcClient,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.dismissConfirmation())
}

/**
 * Replies to a pending tool permission request. The backend owns the pending
 * edit/shell state and validates that the request id is still current.
 */
export async function replyPermission(
  client: ReplIpcClient,
  requestId: string,
  reply: PermissionReply,
): Promise<ReplCommandResult> {
  return assertCommandSucceeded(await client.replyPermission(requestId, reply))
}

function assertCommandSucceeded(
  result: ReplCommandResult,
): ReplCommandResult {
  if (result.error) {
    throw new ReplCommandError(result.error.code, result.error.message)
  }
  return result
}
