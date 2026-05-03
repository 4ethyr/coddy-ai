// __tests__/infrastructure/integration.test.ts
// Integration test: simulates the Electron main→renderer IPC bridge
// without actually spawning Electron or the coddy CLI.
//
// We test the full flow:
//   ReplIpcClient (renderer) ↔ invoke/on ↔ simulated ipcBridge handlers

import { describe, it, expect, beforeEach } from 'vitest'
import type { ReplIpcClient, ReplCommandResult } from '@/domain'
import type {
  ConversationRecord,
  ModelRef,
  ModelRole,
  AssessmentPolicy,
  ReplToolCatalogItem,
  ReplEvent,
  ReplEventEnvelope,
  ReplMode,
  ReplSessionSnapshot,
  ReplSessionSnapshotSession,
  ScreenAssistMode,
} from '@/domain'

// ---------------------------------------------------------------------------
// Simulated IPC bridge — mirrors the production ipcBridge.ts handlers
// ---------------------------------------------------------------------------

/** In-memory store for the simulated daemon */
interface SimDaemon {
  currentSequence: number
  events: ReplEventEnvelope[]
  snapshotSession: ReplSessionSnapshotSession
  toolCatalog: ReplToolCatalogItem[]
  commands: string[]
  history: ConversationRecord[]
}

function createSimDaemon(): SimDaemon {
  return {
    currentSequence: 0,
    events: [],
    commands: [],
    history: [],
    toolCatalog: [
      {
        name: 'filesystem.read_file',
        description: 'Read a UTF-8 text file inside the active workspace',
        category: 'Filesystem',
        input_schema: { type: 'object', required: ['path'] },
        output_schema: { type: 'object' },
        risk_level: 'Low',
        permissions: ['ReadWorkspace'],
        timeout_ms: 5_000,
        approval_policy: 'AutoApprove',
      },
      {
        name: 'shell.run',
        description: 'Execute a workspace-scoped shell command',
        category: 'Shell',
        input_schema: { type: 'object', required: ['command'] },
        output_schema: { type: 'object' },
        risk_level: 'Medium',
        permissions: ['ExecuteCommand'],
        timeout_ms: 30_000,
        approval_policy: 'AskOnUse',
      },
    ],
    snapshotSession: {
      id: 'sim-session-uuid',
      mode: 'FloatingTerminal',
      status: 'Idle',
      policy: 'UnknownAssessment',
      selected_model: { provider: 'ollama', name: 'test-model' },
      voice: { enabled: true, speaking: false, muted: false },
      screen_context: null,
      workspace_context: [],
      messages: [],
      active_run: null,
      pending_permission: null,
      subagent_activity: [],
    } satisfies ReplSessionSnapshotSession,
  }
}

/** Simulates the main process ipcBridge.ts handlers */
function createSimBridge(daemon: SimDaemon) {
  const watchListeners: Array<(data: unknown) => void> = []

  return {
    invoke(channel: string, ...args: unknown[]): Promise<unknown> {
      switch (channel) {
        // ---- Snapshot ----
        case 'repl:snapshot': {
          return Promise.resolve({
            session: daemon.snapshotSession,
            last_sequence: daemon.currentSequence,
          } satisfies ReplSessionSnapshot)
        }

        // ---- Incremental events ----
        case 'repl:events-after': {
          const after = args[0] as number
          const events = daemon.events.filter(
            (e) => e.sequence > after,
          )
          return Promise.resolve({
            events,
            last_sequence: daemon.currentSequence,
          })
        }

        // ---- Tool catalog ----
        case 'repl:tools':
          return Promise.resolve(daemon.toolCatalog)

        case 'repl:history':
          return Promise.resolve(daemon.history)

        case 'repl:eval-multiagent':
          daemon.commands.push('eval-multiagent')
          return Promise.resolve({
            suite: {
              score: 100,
              passed: 2,
              failed: 0,
              reports: [
                { caseName: 'hardness-multiagent', status: 'passed', score: 100 },
                { caseName: 'security-sensitive-routing', status: 'passed', score: 100 },
              ],
            },
            baselineWritten: null,
          })

        case 'repl:eval-prompt-battery':
          daemon.commands.push('eval-prompt-battery')
          return Promise.resolve({
            promptCount: 1200,
            stackCount: 30,
            knowledgeAreaCount: 10,
            passed: 1200,
            failed: 0,
            score: 100,
            memberCoverage: {
              explorer: 1200,
              reviewer: 1200,
              'security-reviewer': 1200,
            },
            failures: [],
          })

        case 'repl:eval-quality':
          daemon.commands.push('eval-quality')
          return Promise.resolve({
            kind: 'coddy.qualityEval',
            version: 1,
            status: 'passed',
            passed: true,
            score: 100,
            checks: [
              {
                name: 'multiagent',
                status: 'passed',
                score: 100,
                passed: 3,
                failed: 0,
              },
              {
                name: 'prompt-battery',
                status: 'passed',
                score: 100,
                promptCount: 1200,
                passed: 1200,
                failed: 0,
              },
              {
                name: 'grounded-response',
                status: 'passed',
                score: 100,
                caseCount: 3,
                passed: 3,
                failed: 0,
              },
            ],
            multiagent: {
              score: 100,
              passed: 3,
              failed: 0,
              reports: [],
            },
            promptBattery: {
              promptCount: 1200,
              stackCount: 30,
              knowledgeAreaCount: 10,
              passed: 1200,
              failed: 0,
              score: 100,
              memberCoverage: {
                explorer: 1200,
              },
              failures: [],
            },
            groundedResponse: {
              kind: 'coddy.groundedResponseEval',
              caseCount: 3,
              passed: 3,
              failed: 0,
              score: 100,
              failures: [],
            },
          })

        // ---- Watch start ----
        case 'repl:watch-start': {
          // Push existing events after sequence
          const after = args[0] as number
          const replay = daemon.events.filter(
            (e) => e.sequence > after,
          )
          for (const event of replay) {
            for (const listener of watchListeners) {
              listener({ event, done: false, streamId: 'sim-stream' })
            }
          }

          return Promise.resolve({ streamId: 'sim-stream' })
        }

        case 'repl:watch-close':
          return Promise.resolve(undefined)

        // ---- Commands ----
        case 'repl:ask': {
          const text = args[0] as string
          const runId = `run-${Date.now()}`
          const assistantText = `Echo: ${text}`
          pushEvent(daemon, watchListeners, {
            MessageAppended: {
              message: {
                id: `msg-${daemon.currentSequence + 1}`,
                role: 'user',
                text,
              },
            },
          })
          pushEvent(daemon, watchListeners, {
            RunStarted: { run_id: runId },
          })
          pushEvent(daemon, watchListeners, {
            IntentDetected: {
              intent: 'AskTechnicalQuestion',
              confidence: 0.8,
            },
          })
          pushEvent(daemon, watchListeners, {
            TokenDelta: { run_id: runId, text: assistantText },
          })
          pushEvent(daemon, watchListeners, {
            MessageAppended: {
              message: {
                id: `msg-${daemon.currentSequence + 1}`,
                role: 'assistant',
                text: assistantText,
              },
            },
          })
          pushEvent(daemon, watchListeners, {
            RunCompleted: { run_id: runId },
          })
          return Promise.resolve({ text: assistantText })
        }

        case 'voice:capture':
          // Simulated voice capture
          pushEvent(daemon, watchListeners, {
            VoiceListeningStarted: {},
          })
          pushEvent(daemon, watchListeners, {
            VoiceTranscriptFinal: { text: 'comando de voz simulado' },
          })
          pushEvent(daemon, watchListeners, {
            IntentDetected: {
              intent: 'SearchDocs',
              confidence: 0.85,
            },
          })
          return Promise.resolve({
            text: 'comando de voz simulado',
          })

        case 'voice:capture-cancel':
          daemon.commands.push('voice-capture-cancel')
          return Promise.resolve({ ok: true })

        case 'repl:stop-speaking':
          daemon.commands.push('stop-speaking')
          return Promise.resolve({ ok: true })

        case 'repl:stop-active-run':
          daemon.commands.push('stop-active-run')
          return Promise.resolve({ ok: true })

        case 'repl:new-session': {
          daemon.commands.push('new-session')
          if (daemon.snapshotSession.messages.length > 0) {
            daemon.history.unshift({
              summary: {
                session_id: daemon.snapshotSession.id,
                title: daemon.snapshotSession.messages[0]?.text ?? 'New conversation',
                created_at_unix_ms: 1,
                updated_at_unix_ms: 2,
                message_count: daemon.snapshotSession.messages.length,
                selected_model: daemon.snapshotSession.selected_model,
                mode: daemon.snapshotSession.mode,
              },
              messages: daemon.snapshotSession.messages,
            })
          }
          daemon.snapshotSession = {
            ...daemon.snapshotSession,
            id: `sim-session-${Date.now()}`,
            status: 'Idle',
            messages: [],
            workspace_context: [],
            active_run: null,
            pending_permission: null,
            subagent_activity: [],
          }
          pushEvent(daemon, watchListeners, {
            SessionStarted: { session_id: daemon.snapshotSession.id },
          })
          return Promise.resolve({ message: 'new session' })
        }

        case 'repl:open-conversation': {
          const sessionId = args[0] as string
          daemon.commands.push(`open-conversation:${sessionId}`)
          const record = daemon.history.find(
            (item) => item.summary.session_id === sessionId,
          )
          if (!record) {
            return Promise.resolve({
              error: {
                code: 'conversation_not_found',
                message: `conversation ${sessionId} was not found`,
              },
            })
          }

          daemon.snapshotSession = {
            ...daemon.snapshotSession,
            id: record.summary.session_id,
            mode: record.summary.mode,
            status: 'Idle',
            selected_model: record.summary.selected_model,
            messages: [],
            workspace_context: [],
            active_run: null,
            pending_permission: null,
            subagent_activity: [],
            streaming_text: '',
          }
          pushEvent(daemon, watchListeners, {
            SessionStarted: { session_id: record.summary.session_id },
          })
          pushEvent(daemon, watchListeners, {
            OverlayShown: { mode: record.summary.mode },
          })
          pushEvent(daemon, watchListeners, {
            ModelSelected: {
              model: record.summary.selected_model,
              role: 'Chat',
            },
          })
          for (const message of record.messages) {
            pushEvent(daemon, watchListeners, {
              MessageAppended: { message },
            })
          }

          return Promise.resolve({ message: 'conversation opened' })
        }

        case 'repl:select-model': {
          const model = args[0] as ModelRef
          const role = args[1] as ModelRole
          daemon.commands.push(
            `select-model:${role}:${model.provider}/${model.name}`,
          )
          pushEvent(daemon, watchListeners, {
            ModelSelected: { model, role },
          })
          if (role === 'Chat') {
            daemon.snapshotSession.selected_model = model
          }
          return Promise.resolve({
            text: `Modelo ${model.provider}/${model.name} selecionado.`,
          })
        }

        case 'repl:open-ui': {
          const mode = args[0] as ReplMode
          daemon.commands.push(`open-ui:${mode}`)
          daemon.snapshotSession.mode = mode
          pushEvent(daemon, watchListeners, {
            OverlayShown: { mode },
          })
          return Promise.resolve({
            text: `Modo ${mode} aberto.`,
          })
        }

        case 'repl:capture-and-explain': {
          const mode = args[0] as ScreenAssistMode
          const policy = args[1] as AssessmentPolicy
          const allowed = !(
            policy === 'UnknownAssessment'
            || (policy === 'RestrictedAssessment' && mode === 'MultipleChoice')
          )
          daemon.commands.push(`capture-and-explain:${mode}:${policy}`)
          pushEvent(daemon, watchListeners, {
            PolicyEvaluated: {
              policy,
              allowed,
            },
          })
          if (policy === 'UnknownAssessment') {
            daemon.snapshotSession.status = 'AwaitingConfirmation'
          }
          if (!allowed && policy === 'RestrictedAssessment') {
            return Promise.resolve({
              error: {
                code: 'assessment_policy_blocked',
                message:
                  'restricted assessments must not receive final answers or complete code',
              },
            })
          }
          return Promise.resolve({
            text: 'CaptureAndExplain solicitado.',
          })
        }

        case 'repl:dismiss-confirmation':
          daemon.commands.push('dismiss-confirmation')
          daemon.snapshotSession.status = 'Idle'
          pushEvent(daemon, watchListeners, {
            ConfirmationDismissed: {},
          })
          return Promise.resolve({
            text: 'Confirmação dispensada.',
          })

        case 'repl:permission-reply': {
          const requestId = args[0] as string
          const reply = args[1] as 'Once' | 'Always' | 'Reject'
          daemon.commands.push(`permission-reply:${requestId}:${reply}`)
          daemon.snapshotSession.pending_permission = null
          daemon.snapshotSession.status = 'Idle'
          pushEvent(daemon, watchListeners, {
            PermissionReplied: { request_id: requestId, reply },
          })
          return Promise.resolve({
            message: `Permissão ${reply} registrada.`,
          })
        }

        default:
          return Promise.reject(new Error(`Unknown channel: ${channel}`))
      }
    },

    on(
      channel: string,
      callback: (...args: unknown[]) => void,
    ): () => void {
      if (channel === 'repl:watch-event') {
        watchListeners.push(callback)
        return () => {
          const idx = watchListeners.indexOf(callback)
          if (idx >= 0) watchListeners.splice(idx, 1)
        }
      }
      return () => {}
    },

    /** Simulate the daemon pushing a live event */
    pushLiveEvent(event: ReplEvent): void {
      pushEvent(daemon, watchListeners, event)
    },
  }
}

function pushEvent(
  daemon: SimDaemon,
  listeners: Array<(data: unknown) => void>,
  event: ReplEvent,
): void {
  applyEventToSnapshot(daemon, event)
  daemon.currentSequence++
  const envelope: ReplEventEnvelope = {
    sequence: daemon.currentSequence,
    session_id: daemon.snapshotSession.id,
    run_id: null,
    captured_at_unix_ms: Date.now(),
    event,
  }
  daemon.events.push(envelope)
  for (const listener of listeners) {
    listener({ event: envelope, done: false, streamId: 'sim-stream' })
  }
}

function applyEventToSnapshot(daemon: SimDaemon, event: ReplEvent): void {
  if ('MessageAppended' in event) {
    daemon.snapshotSession.messages.push(event.MessageAppended.message)
    daemon.snapshotSession.streaming_text = ''
    return
  }

  if ('RunStarted' in event) {
    daemon.snapshotSession.active_run = event.RunStarted.run_id
    daemon.snapshotSession.status = 'Thinking'
    daemon.snapshotSession.streaming_text = ''
    daemon.snapshotSession.subagent_activity = []
    return
  }

  if ('TokenDelta' in event) {
    daemon.snapshotSession.status = 'Streaming'
    daemon.snapshotSession.streaming_text = `${
      daemon.snapshotSession.streaming_text ?? ''
    }${event.TokenDelta.text}`
    return
  }

  if ('RunCompleted' in event) {
    daemon.snapshotSession.active_run = null
    daemon.snapshotSession.status = 'Idle'
    daemon.snapshotSession.streaming_text = ''
    return
  }

  if ('SubagentLifecycleUpdated' in event) {
    const update = event.SubagentLifecycleUpdated.update
    const activityId = `${update.name}:${update.mode}`
    const current = (daemon.snapshotSession.subagent_activity ?? []) as Array<{
      id: string
      required_output_fields?: string[]
      output_additional_properties_allowed?: boolean
    }>
    const existingIndex = current.findIndex((item) => item.id === activityId)
    const existingActivity = existingIndex >= 0 ? current[existingIndex] : undefined
    const activity = {
      ...update,
      id: activityId,
      required_output_fields: existingActivity?.required_output_fields ?? [],
      output_additional_properties_allowed:
        existingActivity?.output_additional_properties_allowed ?? true,
    }
    daemon.snapshotSession.subagent_activity =
      existingIndex >= 0
        ? current.map((item, index) => (index === existingIndex ? activity : item))
        : [...current, activity]
  }
}

// ---------------------------------------------------------------------------
// SimElectronReplIpcClient — uses the sim bridge instead of window.replApi
// ---------------------------------------------------------------------------

function createSimClient(sim: ReturnType<typeof createSimBridge>): ReplIpcClient {
  return {
    async getSnapshot() {
      return (await sim.invoke('repl:snapshot')) as ReplSessionSnapshot
    },

    async getEventsAfter(afterSequence: number) {
      const raw = (await sim.invoke('repl:events-after', afterSequence)) as {
        events: ReplEventEnvelope[]
        last_sequence: number
      }
      return { events: raw.events, lastSequence: raw.last_sequence }
    },

    async getToolCatalog() {
      return (await sim.invoke('repl:tools')) as ReplToolCatalogItem[]
    },

    async getConversationHistory() {
      return (await sim.invoke('repl:history')) as Awaited<
        ReturnType<ReplIpcClient['getConversationHistory']>
      >
    },

    async getActiveWorkspace() {
      return { path: null }
    },

    async selectWorkspaceFolder() {
      return { path: '/tmp/coddy-workspace' }
    },

    async runMultiagentEval(request = {}) {
      return (await sim.invoke('repl:eval-multiagent', request)) as Awaited<
        ReturnType<ReplIpcClient['runMultiagentEval']>
      >
    },

    async runPromptBatteryEval() {
      return (await sim.invoke('repl:eval-prompt-battery')) as Awaited<
        ReturnType<ReplIpcClient['runPromptBatteryEval']>
      >
    },

    async runQualityEval() {
      return (await sim.invoke('repl:eval-quality')) as Awaited<
        ReturnType<ReplIpcClient['runQualityEval']>
      >
    },

    async listProviderModels(request) {
      return {
        provider: request.provider,
        models: [],
        source: request.provider === 'ollama' ? 'local' : 'api',
        fetchedAtUnixMs: 1,
      }
    },

    watchEvents(afterSequence: number): AsyncIterable<ReplEventEnvelope> {
      const stream: AsyncIterable<ReplEventEnvelope> = {
        [Symbol.asyncIterator]() {
          let done = false
          const pending: ReplEventEnvelope[] = []
          let resolveNext:
            | ((value: IteratorResult<ReplEventEnvelope>) => void)
            | null = null

          const unsubscribe = sim.on('repl:watch-event', (data: unknown) => {
            const payload = data as {
              event?: ReplEventEnvelope
              done?: boolean
            }
            if (payload.done) {
              done = true
              resolveNext?.({ done: true, value: undefined })
              return
            }
            if (payload.event) {
              if (resolveNext) {
                resolveNext({ done: false, value: payload.event })
                resolveNext = null
              } else {
                pending.push(payload.event)
              }
            }
          })

          // Replay events after the given sequence from the store
          void sim.invoke('repl:watch-start', afterSequence)

          return {
            async next(): Promise<IteratorResult<ReplEventEnvelope>> {
              if (done && pending.length === 0) {
                return { done: true, value: undefined }
              }
              if (pending.length > 0) {
                return { done: false, value: pending.shift()! }
              }
              return new Promise((resolve) => {
                resolveNext = resolve
              })
            },
            async return(): Promise<IteratorResult<ReplEventEnvelope>> {
              done = true
              unsubscribe()
              sim.invoke('repl:watch-close')
              return { done: true, value: undefined }
            },
          }
        },
      }
      return stream
    },

    async ask(text: string) {
      return (await sim.invoke('repl:ask', text)) as ReplCommandResult
    },

    async voiceTurn(transcript: string) {
      return { text: transcript }
    },

    async stopActiveRun() {
      await sim.invoke('repl:stop-active-run')
    },

    async newSession() {
      return (await sim.invoke('repl:new-session')) as ReplCommandResult
    },

    async openConversation(sessionId: string) {
      return (await sim.invoke(
        'repl:open-conversation',
        sessionId,
      )) as ReplCommandResult
    },

    async stopSpeaking() {
      await sim.invoke('repl:stop-speaking')
    },

    async selectModel(model: ModelRef, role: ModelRole) {
      return (await sim.invoke(
        'repl:select-model',
        model,
        role,
      )) as ReplCommandResult
    },

    async openUi(mode: ReplMode) {
      return (await sim.invoke('repl:open-ui', mode)) as ReplCommandResult
    },

    async captureAndExplain(
      mode: ScreenAssistMode,
      policy: AssessmentPolicy,
    ) {
      return (await sim.invoke(
        'repl:capture-and-explain',
        mode,
        policy,
      )) as ReplCommandResult
    },

    async dismissConfirmation() {
      return (await sim.invoke(
        'repl:dismiss-confirmation',
      )) as ReplCommandResult
    },

    async replyPermission(requestId: string, reply: 'Once' | 'Always' | 'Reject') {
      return (await sim.invoke(
        'repl:permission-reply',
        requestId,
        reply,
      )) as ReplCommandResult
    },

    async captureVoice(options = {}) {
      return (await sim.invoke('voice:capture', options)) as ReplCommandResult
    },

    async cancelVoiceCapture() {
      await sim.invoke('voice:capture-cancel')
    },
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('IPC integration', () => {
  let daemon: SimDaemon
  let sim: ReturnType<typeof createSimBridge>
  let client: ReplIpcClient

  beforeEach(() => {
    daemon = createSimDaemon()
    sim = createSimBridge(daemon)
    client = createSimClient(sim)
  })

  describe('snapshot', () => {
    it('returns session with last_sequence', async () => {
      const snapshot = await client.getSnapshot()
      expect(snapshot.session).toBeDefined()
      expect(typeof snapshot.last_sequence).toBe('number')
      expect(snapshot.session.id).toBe('sim-session-uuid')
    })
  })

  describe('events after', () => {
    it('returns only events after the given sequence', async () => {
      // Push some events
      sim.pushLiveEvent({ VoiceListeningStarted: {} })
      sim.pushLiveEvent({
        VoiceTranscriptFinal: { text: 'hello' },
      })

      const batch = await client.getEventsAfter(1)
      expect(batch.events).toHaveLength(1)
      expect(batch.lastSequence).toBe(2)
    })

    it('returns empty for no new events', async () => {
      sim.pushLiveEvent({ VoiceListeningStarted: {} })
      const batch = await client.getEventsAfter(1)
      expect(batch.events).toHaveLength(0)
    })

    it('applies subagent lifecycle events into the session snapshot', async () => {
      sim.pushLiveEvent({
        SubagentLifecycleUpdated: {
          update: {
            name: 'eval-runner',
            mode: 'evaluation',
            status: 'Prepared',
            readiness_score: 100,
            reason: null,
          },
        },
      })

      const snapshot = await client.getSnapshot()

      expect(snapshot.session.subagent_activity).toEqual([
        {
          id: 'eval-runner:evaluation',
          name: 'eval-runner',
          mode: 'evaluation',
          status: 'Prepared',
          readiness_score: 100,
          required_output_fields: [],
          output_additional_properties_allowed: true,
          reason: null,
        },
      ])
    })
  })

  describe('tool catalog', () => {
    it('returns backend tool metadata through the frontend client port', async () => {
      const tools = await client.getToolCatalog()

      expect(tools.map((tool) => tool.name)).toEqual([
        'filesystem.read_file',
        'shell.run',
      ])
      expect(tools[0]).toMatchObject({
        category: 'Filesystem',
        input_schema: { type: 'object', required: ['path'] },
        output_schema: { type: 'object' },
        risk_level: 'Low',
        permissions: ['ReadWorkspace'],
        approval_policy: 'AutoApprove',
      })
      expect(tools[1]).toMatchObject({
        category: 'Shell',
        input_schema: { type: 'object', required: ['command'] },
        output_schema: { type: 'object' },
        risk_level: 'Medium',
        permissions: ['ExecuteCommand'],
        approval_policy: 'AskOnUse',
      })
    })
  })

  describe('multiagent eval', () => {
    it('runs the deterministic multiagent eval through the frontend client port', async () => {
      const result = await client.runMultiagentEval({
        baseline: 'evals/baselines/main.json',
      })

      expect(result.suite).toMatchObject({
        score: 100,
        passed: 2,
        failed: 0,
      })
      expect(result.suite.reports).toHaveLength(2)
      expect(daemon.commands).toEqual(['eval-multiagent'])
    })

    it('runs the deterministic prompt battery through the frontend client port', async () => {
      const result = await client.runPromptBatteryEval()

      expect(result).toMatchObject({
        promptCount: 1200,
        stackCount: 30,
        knowledgeAreaCount: 10,
        score: 100,
        passed: 1200,
        failed: 0,
      })
      expect(result.memberCoverage.explorer).toBe(1200)
      expect(daemon.commands).toEqual(['eval-prompt-battery'])
    })

    it('runs the combined deterministic quality gate through the frontend client port', async () => {
      const result = await client.runQualityEval()

      expect(result).toMatchObject({
        kind: 'coddy.qualityEval',
        version: 1,
        status: 'passed',
        passed: true,
        score: 100,
      })
      expect(result.checks).toHaveLength(3)
      expect(result.multiagent.passed).toBe(3)
      expect(result.promptBattery.promptCount).toBe(1200)
      expect(result.groundedResponse?.caseCount).toBe(3)
      expect(daemon.commands).toEqual(['eval-quality'])
    })
  })

  describe('ask command', () => {
    it('sends text and returns result', async () => {
      const result = await client.ask('quem foi rousseau?')
      expect(result.text).toContain('rousseau')
    })

    it('streams and persists user and assistant messages through events', async () => {
      const result = await client.ask('liste os arquivos')
      const snapshot = await client.getSnapshot()
      const batch = await client.getEventsAfter(0)

      expect(result.text).toBe('Echo: liste os arquivos')
      expect(
        snapshot.session.messages.map((message) => [
          message.role,
          message.text,
        ]),
      ).toEqual([
        ['user', 'liste os arquivos'],
        ['assistant', 'Echo: liste os arquivos'],
      ])
      expect(snapshot.session.active_run).toBeNull()
      expect(snapshot.session.status).toBe('Idle')
      expect(batch.events.map((event) => Object.keys(event.event)[0])).toEqual([
        'MessageAppended',
        'RunStarted',
        'IntentDetected',
        'TokenDelta',
        'MessageAppended',
        'RunCompleted',
      ])
    })
  })

  describe('conversation history', () => {
    it('opens a selected history item and restores its session snapshot', async () => {
      daemon.history = [
        {
          summary: {
            session_id: 'history-session-1',
            title: 'Analyze workspace',
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
            message_count: 2,
            selected_model: {
              provider: 'openrouter',
              name: 'deepseek/deepseek-v4-flash',
            },
            mode: 'DesktopApp',
          },
          messages: [
            { id: 'm1', role: 'user', text: 'Analise a codebase' },
            { id: 'm2', role: 'assistant', text: 'Resumo da arquitetura' },
          ],
        },
      ]

      const result = await client.openConversation('history-session-1')
      const snapshot = await client.getSnapshot()

      expect(result.message).toBe('conversation opened')
      expect(snapshot.session.id).toBe('history-session-1')
      expect(snapshot.session.mode).toBe('DesktopApp')
      expect(snapshot.session.selected_model).toEqual({
        provider: 'openrouter',
        name: 'deepseek/deepseek-v4-flash',
      })
      expect(snapshot.session.messages.map((message) => message.text)).toEqual([
        'Analise a codebase',
        'Resumo da arquitetura',
      ])
      expect(daemon.commands).toEqual([
        'open-conversation:history-session-1',
      ])
    })
  })

  describe('voice capture', () => {
    it('captures and returns transcript', async () => {
      const result = await client.captureVoice()
      expect(result.text).toBe('comando de voz simulado')
    })

    it('routes voice capture cancellation to the capture channel', async () => {
      await client.cancelVoiceCapture()

      expect(daemon.commands).toEqual(['voice-capture-cancel'])
    })
  })

  describe('stop commands', () => {
    it('routes stopSpeaking to the speech cancellation channel', async () => {
      await client.stopSpeaking()

      expect(daemon.commands).toEqual(['stop-speaking'])
    })

    it('routes stopActiveRun to the run cancellation channel', async () => {
      await client.stopActiveRun()

      expect(daemon.commands).toEqual(['stop-active-run'])
    })
  })

  describe('model and UI commands', () => {
    it('selects the chat model and emits a ModelSelected event', async () => {
      const model = { provider: 'ollama', name: 'qwen2.5:0.5b' }

      const result = await client.selectModel(model, 'Chat')
      const snapshot = await client.getSnapshot()

      expect(result.text).toContain('qwen2.5')
      expect(snapshot.session.selected_model).toEqual(model)
      expect(daemon.commands).toEqual([
        'select-model:Chat:ollama/qwen2.5:0.5b',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        ModelSelected: { model, role: 'Chat' },
      })
    })

    it('opens desktop UI mode and stores it in the snapshot', async () => {
      const result = await client.openUi('DesktopApp')
      const snapshot = await client.getSnapshot()

      expect(result.text).toContain('DesktopApp')
      expect(snapshot.session.mode).toBe('DesktopApp')
      expect(daemon.commands).toEqual(['open-ui:DesktopApp'])
      expect(daemon.events.at(-1)?.event).toEqual({
        OverlayShown: { mode: 'DesktopApp' },
      })
    })

    it('routes screen assist through policy evaluation events', async () => {
      const result = await client.captureAndExplain(
        'ExplainVisibleScreen',
        'UnknownAssessment',
      )

      expect(result.text).toContain('CaptureAndExplain')
      expect(daemon.commands).toEqual([
        'capture-and-explain:ExplainVisibleScreen:UnknownAssessment',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        PolicyEvaluated: {
          policy: 'UnknownAssessment',
          allowed: false,
        },
      })
    })

    it('dismisses a pending confirmation through a structured event', async () => {
      await client.captureAndExplain(
        'ExplainVisibleScreen',
        'UnknownAssessment',
      )

      const result = await client.dismissConfirmation()
      const snapshot = await client.getSnapshot()

      expect(result.text).toContain('dispensada')
      expect(snapshot.session.status).toBe('Idle')
      expect(daemon.commands).toEqual([
        'capture-and-explain:ExplainVisibleScreen:UnknownAssessment',
        'dismiss-confirmation',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        ConfirmationDismissed: {},
      })
    })

    it('returns structured policy errors instead of throwing for blocked screen assist', async () => {
      const result = await client.captureAndExplain(
        'MultipleChoice',
        'RestrictedAssessment',
      )

      expect(result.error).toEqual({
        code: 'assessment_policy_blocked',
        message:
          'restricted assessments must not receive final answers or complete code',
      })
      expect(daemon.commands).toEqual([
        'capture-and-explain:MultipleChoice:RestrictedAssessment',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        PolicyEvaluated: {
          policy: 'RestrictedAssessment',
          allowed: false,
        },
      })
    })
  })

  describe('event stream', () => {
    it('receives live events after subscription', async () => {
      const received: ReplEventEnvelope[] = []

      const stream = client.watchEvents(0)
      const iterator = stream[Symbol.asyncIterator]()

      // Give the stream a tick to set up listeners
      await new Promise((r) => setTimeout(r, 10))

      // Push live events
      sim.pushLiveEvent({
        VoiceTranscriptFinal: { text: 'terminal' },
      })

      const first = await iterator.next()
      expect(first.done).toBe(false)
      if (!first.done && first.value) {
        received.push(first.value)
      }

      expect(received).toHaveLength(1)
    })
  })
})
