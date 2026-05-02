// presentation/hooks/useSession.ts
// Hook: manages the full REPL session lifecycle.
// Loads snapshot, starts event stream, exposes state + actions.

import { useState, useEffect, useCallback, useRef } from 'react'
import type {
  ModelRef,
  ModelProviderListRequest,
  ModelProviderListResult,
  ModelRole,
  MultiagentEvalRequest,
  MultiagentEvalResult,
  PermissionReply,
  PromptBatteryResult,
  QualityEvalResult,
  ConversationRecord,
  ReplCommandResult,
  ReplMode,
  ReplSession,
  ReplToolCatalogItem,
  VoiceCaptureOptions,
  WorkspaceSelectionResult,
  AssessmentPolicy,
  ScreenAssistMode,
} from '@/domain'
import type { SessionState } from '@/application'
import {
  initializeSession,
  createLocalSession,
  startEventStream,
  sendAsk,
  cancelRun,
  cancelSpeech,
  openConversation,
  startNewSession,
  selectModel,
  openUi,
  captureVoice,
  cancelVoiceCapture,
  captureAndExplain,
  dismissConfirmation,
  replyPermission,
  listProviderModels,
  runMultiagentEval,
  runPromptBatteryEval,
  runQualityEval,
  getActiveWorkspace,
  loadConversationHistory,
  selectWorkspaceFolder,
  loadSettings,
} from '@/application'
import { useReplClient } from './useReplClient'

export type EvalRunStatus = 'idle' | 'running' | 'succeeded' | 'failed'
export type MultiagentEvalStatus = EvalRunStatus

export interface UseSessionReturn {
  session: ReplSession
  lastSequence: number
  toolCatalog: ReplToolCatalogItem[]
  multiagentEval: MultiagentEvalResult | null
  multiagentEvalStatus: EvalRunStatus
  multiagentEvalError: string | null
  promptBattery: PromptBatteryResult | null
  promptBatteryStatus: EvalRunStatus
  promptBatteryError: string | null
  qualityEval: QualityEvalResult | null
  qualityEvalStatus: EvalRunStatus
  qualityEvalError: string | null
  activeWorkspacePath: string | null
  conversationHistory: ConversationRecord[]
  conversationHistoryStatus: EvalRunStatus
  conversationHistoryError: string | null
  workspaceSelectionStatus: EvalRunStatus
  workspaceSelectionError: string | null
  /** True while still connecting / loading the first snapshot */
  connecting: boolean
  /** True when the daemon stream disconnected and we're retrying */
  reconnecting: boolean
  error: string | null

  /** Send a text question */
  ask: (text: string) => Promise<void>

  /** Stop the current generation */
  cancelRun: () => Promise<void>

  /** Archive the current conversation and start a clean session */
  newSession: () => Promise<void>

  /** Restore a persisted conversation as the active session */
  openConversation: (sessionId: string) => Promise<void>

  /** Stop TTS playback */
  cancelSpeech: () => Promise<void>

  /** Select a model for the requested role */
  selectModel: (model: ModelRef, role?: ModelRole) => Promise<void>

  /** Load available models for a provider with a request-scoped credential */
  listProviderModels: (
    request: ModelProviderListRequest,
  ) => Promise<ModelProviderListResult>

  /** Run the deterministic multiagent harness exposed by the backend */
  runMultiagentEval: (
    request?: MultiagentEvalRequest,
  ) => Promise<MultiagentEvalResult>

  /** Run the deterministic 1,200-prompt routing battery exposed by the backend */
  runPromptBatteryEval: () => Promise<PromptBatteryResult>

  /** Run the combined deterministic quality gate exposed by the backend */
  runQualityEval: () => Promise<QualityEvalResult>

  /** Select the local filesystem workspace used by the Rust runtime */
  selectWorkspaceFolder: () => Promise<WorkspaceSelectionResult>

  /** Load persisted redacted conversation history */
  loadConversationHistory: () => Promise<ConversationRecord[]>

  /** Switch the REPL UI mode through the daemon */
  openUi: (mode: ReplMode) => Promise<void>

  /** Capture one voice turn; the backend dispatches the transcript itself */
  captureVoice: (options?: VoiceCaptureOptions) => Promise<ReplCommandResult>

  /** Cancel the active microphone capture */
  cancelVoiceCapture: () => Promise<void>

  /** Start a policy-aware screen assistance flow */
  captureAndExplain: (
    mode: ScreenAssistMode,
    policy?: AssessmentPolicy,
  ) => Promise<void>

  /** Dismiss a pending policy confirmation without sending prompt text */
  dismissConfirmation: () => Promise<void>

  /** Reply to a pending tool permission request */
  replyPermission: (requestId: string, reply: PermissionReply) => Promise<void>

  /** Manually retry connection to the daemon */
  reconnect: () => void
}

export function useSession(): UseSessionReturn {
  const client = useReplClient()
  const [state, setState] = useState<SessionState>(createLocalSession())
  const [connecting, setConnecting] = useState(true)
  const [reconnecting, setReconnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [multiagentEval, setMultiagentEval] =
    useState<MultiagentEvalResult | null>(null)
  const [multiagentEvalStatus, setMultiagentEvalStatus] =
    useState<EvalRunStatus>('idle')
  const [multiagentEvalError, setMultiagentEvalError] =
    useState<string | null>(null)
  const [promptBattery, setPromptBattery] =
    useState<PromptBatteryResult | null>(null)
  const [promptBatteryStatus, setPromptBatteryStatus] =
    useState<EvalRunStatus>('idle')
  const [promptBatteryError, setPromptBatteryError] =
    useState<string | null>(null)
  const [qualityEval, setQualityEval] = useState<QualityEvalResult | null>(null)
  const [qualityEvalStatus, setQualityEvalStatus] =
    useState<EvalRunStatus>('idle')
  const [qualityEvalError, setQualityEvalError] = useState<string | null>(null)
  const [activeWorkspacePath, setActiveWorkspacePath] = useState<string | null>(
    null,
  )
  const [conversationHistory, setConversationHistory] = useState<
    ConversationRecord[]
  >([])
  const [conversationHistoryStatus, setConversationHistoryStatus] =
    useState<EvalRunStatus>('idle')
  const [conversationHistoryError, setConversationHistoryError] = useState<
    string | null
  >(null)
  const [workspaceSelectionStatus, setWorkspaceSelectionStatus] =
    useState<EvalRunStatus>('idle')
  const [workspaceSelectionError, setWorkspaceSelectionError] =
    useState<string | null>(null)
  const abortRef = useRef<(() => void) | null>(null)
  const initCountRef = useRef(0)

  // Initialize: fetch snapshot, then start watching
  const init = useCallback(() => {
    abortRef.current?.()

    const count = ++initCountRef.current
    let cancelled = false

    setConnecting(true)
    setError(null)

    void (async () => {
      try {
        const [initial, workspace] = await Promise.all([
          initializeSession(client),
          getActiveWorkspace(client).catch(() => ({ path: null })),
        ])
        if (cancelled || count !== initCountRef.current) return

        setState(initial)
        setActiveWorkspacePath(workspace.path)
        setConnecting(false)

        // Start live event stream
        abortRef.current = startEventStream(
          client,
          initial,
          (newState) => {
            if (!cancelled && count === initCountRef.current) {
              setState(newState)
              setReconnecting(false)
            }
          },
          (err) => {
            if (!cancelled && count === initCountRef.current) {
              setError(err.message)
              setReconnecting(true)
            }
          },
        )
      } catch (err) {
        if (!cancelled && count === initCountRef.current) {
          const msg = err instanceof Error ? err.message : String(err)
          setError(msg)
          setConnecting(false)
          setReconnecting(true)
        }
      }
    })()

    return () => {
      cancelled = true
    }
  }, [client])

  // Initial load
  useEffect(() => {
    const cleanup = init()
    return () => {
      cleanup?.()
      abortRef.current?.()
    }
  }, [init])

  const ask = useCallback(
    async (text: string) => {
      try {
        await sendAsk(client, text)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  const handleCancelRun = useCallback(async () => {
    try {
      await cancelRun(client)
    } catch (err) {
      setError(actionErrorMessage('Coddy could not stop the active run', err))
    }
  }, [client])

  const handleNewSession = useCallback(async () => {
    try {
      await startNewSession(client)
      setConversationHistoryStatus('idle')
      init()
    } catch (err) {
      setError(actionErrorMessage('Coddy could not start a new session', err))
    }
  }, [client, init])

  const handleOpenConversation = useCallback(
    async (sessionId: string) => {
      try {
        await openConversation(client, sessionId)
        setConversationHistoryStatus('idle')
        init()
      } catch (err) {
        setError(actionErrorMessage('Coddy could not open conversation', err))
      }
    },
    [client, init],
  )

  const handleCancelSpeech = useCallback(async () => {
    try {
      await cancelSpeech(client)
    } catch (err) {
      setError(actionErrorMessage('Coddy could not stop speech playback', err))
    }
  }, [client])

  const handleSelectModel = useCallback(
    async (model: ModelRef, role: ModelRole = 'Chat') => {
      try {
        await selectModel(client, model, role, {
          localProviderPreference:
            loadSettings().localModel.providerPreference,
        })
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  const handleListProviderModels = useCallback(
    async (request: ModelProviderListRequest) => {
      return listProviderModels(client, request)
    },
    [client],
  )

  const handleRunMultiagentEval = useCallback(
    async (request: MultiagentEvalRequest = {}) => {
      setMultiagentEvalStatus('running')
      setMultiagentEvalError(null)

      try {
        const result = await runMultiagentEval(client, request)
        setMultiagentEval(result)
        setMultiagentEvalStatus(
          result.comparison?.status === 'failed' || result.suite.failed > 0
            ? 'failed'
            : 'succeeded',
        )
        return result
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setMultiagentEvalError(message)
        setMultiagentEvalStatus('failed')
        setError(message)
        throw err
      }
    },
    [client],
  )

  const handleRunPromptBatteryEval = useCallback(async () => {
    setPromptBatteryStatus('running')
    setPromptBatteryError(null)

    try {
      const result = await runPromptBatteryEval(client)
      setPromptBattery(result)
      setPromptBatteryStatus(result.failed > 0 ? 'failed' : 'succeeded')
      return result
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setPromptBatteryError(message)
      setPromptBatteryStatus('failed')
      setError(message)
      throw err
    }
  }, [client])

  const handleRunQualityEval = useCallback(async () => {
    setQualityEvalStatus('running')
    setQualityEvalError(null)

    try {
      const result = await runQualityEval(client)
      setQualityEval(result)
      setQualityEvalStatus(result.passed ? 'succeeded' : 'failed')
      return result
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setQualityEvalError(message)
      setQualityEvalStatus('failed')
      setError(message)
      throw err
    }
  }, [client])

  const handleSelectWorkspaceFolder = useCallback(async () => {
    setWorkspaceSelectionStatus('running')
    setWorkspaceSelectionError(null)

    try {
      const result = await selectWorkspaceFolder(client)
      setActiveWorkspacePath(result.path)
      setWorkspaceSelectionStatus('succeeded')
      if (!result.cancelled) {
        init()
      }
      return result
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setWorkspaceSelectionError(message)
      setWorkspaceSelectionStatus('failed')
      setError(message)
      throw err
    }
  }, [client, init])

  const handleLoadConversationHistory = useCallback(async () => {
    setConversationHistoryStatus('running')
    setConversationHistoryError(null)

    try {
      const result = await loadConversationHistory(client)
      setConversationHistory(result)
      setConversationHistoryStatus('succeeded')
      return result
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setConversationHistoryError(message)
      setConversationHistoryStatus('failed')
      setError(message)
      throw err
    }
  }, [client])

  const handleOpenUi = useCallback(
    async (mode: ReplMode) => {
      try {
        await openUi(client, mode)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  const handleCaptureVoice = useCallback(
    async (
      options: VoiceCaptureOptions = {},
    ): Promise<ReplCommandResult> => {
      try {
        return await captureVoice(client, options)
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setError(message)
        return { error: { code: 'VOICE_CAPTURE_FAILED', message } }
      }
    },
    [client],
  )

  const handleCancelVoiceCapture = useCallback(async () => {
    try {
      await cancelVoiceCapture(client)
    } catch (err) {
      setError(actionErrorMessage('Coddy could not cancel voice capture', err))
    }
  }, [client])

  const handleCaptureAndExplain = useCallback(
    async (
      mode: ScreenAssistMode,
      policy: AssessmentPolicy = state.session.policy,
    ) => {
      try {
        await captureAndExplain(client, mode, policy)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client, state.session.policy],
  )

  const handleDismissConfirmation = useCallback(async () => {
    try {
      await dismissConfirmation(client)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [client])

  const handleReplyPermission = useCallback(
    async (requestId: string, reply: PermissionReply) => {
      try {
        await replyPermission(client, requestId, reply)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  return {
    session: state.session,
    lastSequence: state.lastSequence,
    toolCatalog: state.toolCatalog,
    multiagentEval,
    multiagentEvalStatus,
    multiagentEvalError,
    promptBattery,
    promptBatteryStatus,
    promptBatteryError,
    qualityEval,
    qualityEvalStatus,
    qualityEvalError,
    activeWorkspacePath,
    conversationHistory,
    conversationHistoryStatus,
    conversationHistoryError,
    workspaceSelectionStatus,
    workspaceSelectionError,
    connecting,
    reconnecting,
    error,
    ask,
    cancelRun: handleCancelRun,
    newSession: handleNewSession,
    openConversation: handleOpenConversation,
    cancelSpeech: handleCancelSpeech,
    selectModel: handleSelectModel,
    listProviderModels: handleListProviderModels,
    runMultiagentEval: handleRunMultiagentEval,
    runPromptBatteryEval: handleRunPromptBatteryEval,
    runQualityEval: handleRunQualityEval,
    loadConversationHistory: handleLoadConversationHistory,
    selectWorkspaceFolder: handleSelectWorkspaceFolder,
    openUi: handleOpenUi,
    captureVoice: handleCaptureVoice,
    cancelVoiceCapture: handleCancelVoiceCapture,
    captureAndExplain: handleCaptureAndExplain,
    dismissConfirmation: handleDismissConfirmation,
    replyPermission: handleReplyPermission,
    reconnect: init,
  }
}

function actionErrorMessage(action: string, err: unknown): string {
  const detail = err instanceof Error ? err.message : String(err)
  return detail ? `${action}: ${detail}` : action
}
