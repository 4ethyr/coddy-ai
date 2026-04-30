import { describe, it, expect } from 'vitest'
import type { ReplEvent, ReplEventEnvelope, ReplIntent, ToolStatus, ShortcutSource, ExtractionSource } from '@/domain/types/events'
import type { SessionStatus, AssessmentPolicy } from '@/domain/types/session'
import type { RequestedHelp, AssistanceFallback } from '@/domain/types/policy'

describe('Domain type contracts', () => {
  describe('ReplEvent discriminated union', () => {
    it('all event variants are valid ReplEvent types', () => {
      const events: { [K in keyof ReplEvent]: ReplEvent } = {
        SessionStarted: { SessionStarted: { session_id: 'uuid' } },
        RunStarted: { RunStarted: { run_id: 'uuid' } },
        ShortcutTriggered: { ShortcutTriggered: { binding: 'Ctrl+Space', source: 'Cli' as ShortcutSource } },
        OverlayShown: { OverlayShown: { mode: 'FloatingTerminal' } },
        VoiceListeningStarted: { VoiceListeningStarted: {} },
        VoiceTranscriptPartial: { VoiceTranscriptPartial: { text: 'hello' } },
        VoiceTranscriptFinal: { VoiceTranscriptFinal: { text: 'hello world' } },
        ScreenCaptured: { ScreenCaptured: { source: 'ScreenshotOcr' as ExtractionSource, bytes: 1024 } },
        OcrCompleted: { OcrCompleted: { chars: 500 } },
        IntentDetected: { IntentDetected: { intent: 'OpenApplication' as ReplIntent, confidence: 0.95 } },
        PolicyEvaluated: { PolicyEvaluated: { policy: 'Practice', allowed: true } },
        ConfirmationDismissed: { ConfirmationDismissed: {} },
        ModelSelected: {
          ModelSelected: {
            model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
            role: 'Chat',
          },
        },
        SearchStarted: { SearchStarted: { query: 'Rust docs', provider: 'google' } },
        SearchContextExtracted: { SearchContextExtracted: { provider: 'google', organic_results: 5, ai_overview_present: false } },
        ContextItemAdded: {
          ContextItemAdded: {
            item: {
              id: 'tool:filesystem.read_file:src/main.rs',
              label: 'filesystem.read_file: src/main.rs',
              sensitive: false,
            },
          },
        },
        TokenDelta: { TokenDelta: { run_id: 'run-1', text: 'Hello' } },
        MessageAppended: { MessageAppended: { message: { id: 'm1', role: 'user', text: 'hi' } } },
        ToolStarted: { ToolStarted: { name: 'search_web' } },
        ToolCompleted: { ToolCompleted: { name: 'search_web', status: 'Denied' as ToolStatus } },
        SubagentRouted: {
          SubagentRouted: {
            recommendations: [
              {
                name: 'eval-runner',
                score: 88,
                mode: 'evaluation',
                matched_signals: ['eval', 'harness'],
              },
            ],
          },
        },
        SubagentHandoffPrepared: {
          SubagentHandoffPrepared: {
            handoff: {
              name: 'eval-runner',
              mode: 'evaluation',
              approval_required: true,
              allowed_tools: ['filesystem.read_file', 'shell.run'],
              required_output_fields: ['score', 'passed'],
              output_additional_properties_allowed: false,
              timeout_ms: 60000,
              max_context_tokens: 8000,
              validation_checklist: ['Use deterministic checks'],
              safety_notes: ['Do not expose secrets'],
              readiness_score: 100,
              readiness_issues: [],
            },
          },
        },
        SubagentLifecycleUpdated: {
          SubagentLifecycleUpdated: {
            update: {
              name: 'eval-runner',
              mode: 'evaluation',
              status: 'Prepared',
              readiness_score: 100,
              reason: null,
            },
          },
        },
        PermissionRequested: {
          PermissionRequested: {
            request: {
              id: 'perm-1',
              session_id: 'session-1',
              run_id: 'run-1',
              tool_call_id: 'tool-call-1',
              tool_name: 'filesystem.apply_edit',
              permission: 'WriteWorkspace',
              patterns: ['src/App.tsx'],
              risk_level: 'High',
              metadata: { path: 'src/App.tsx' },
              requested_at_unix_ms: 1775000000000,
            },
          },
        },
        PermissionReplied: {
          PermissionReplied: {
            request_id: 'perm-1',
            reply: 'Once',
          },
        },
        TtsQueued: { TtsQueued: {} },
        TtsStarted: { TtsStarted: {} },
        TtsCompleted: { TtsCompleted: {} },
        RunCompleted: { RunCompleted: { run_id: 'uuid' } },
        Error: { Error: { code: 'E001', message: 'Something went wrong' } },
      }

      expect(Object.keys(events)).toHaveLength(30)

      // Verify each event is correctly typed
      for (const [key, event] of Object.entries(events)) {
        expect(Object.keys(event as object)).toHaveLength(1)
        expect(Object.keys(event as object)[0]).toBe(key)
      }
    })
  })

  describe('ReplEventEnvelope', () => {
    it('has the correct shape', () => {
      const envelope: ReplEventEnvelope = {
        sequence: 1,
        session_id: 'uuid-session',
        run_id: 'uuid-run',
        captured_at_unix_ms: 1775000000000,
        event: { SessionStarted: { session_id: 'uuid' } },
      }

      expect(envelope.sequence).toBe(1)
      expect(envelope.session_id).toBe('uuid-session')
      expect(envelope.run_id).toBe('uuid-run')
      expect(envelope.captured_at_unix_ms).toBeGreaterThan(0)
    })

    it('allows null run_id', () => {
      const envelope: ReplEventEnvelope = {
        sequence: 1,
        session_id: 'uuid',
        run_id: null,
        captured_at_unix_ms: 0,
        event: { SessionStarted: { session_id: 'uuid' } },
      }

      expect(envelope.run_id).toBeNull()
    })
  })

  describe('SessionStatus', () => {
    it('has all 11 values', () => {
      const statuses: SessionStatus[] = [
        'Idle', 'Listening', 'Transcribing', 'CapturingScreen',
        'BuildingContext', 'Thinking', 'Streaming', 'Speaking',
        'AwaitingConfirmation', 'AwaitingToolApproval', 'Error',
      ]
      expect(statuses).toHaveLength(11)
      expect(new Set(statuses).size).toBe(11) // all unique
    })
  })

  describe('AssessmentPolicy', () => {
    it('has all 5 values', () => {
      const policies: AssessmentPolicy[] = [
        'Practice', 'PermittedAi', 'SyntaxOnly',
        'RestrictedAssessment', 'UnknownAssessment',
      ]
      expect(policies).toHaveLength(5)
      expect(new Set(policies).size).toBe(5)
    })
  })

  describe('RequestedHelp', () => {
    it('has all 5 values', () => {
      const helpTypes: RequestedHelp[] = [
        'ExplainConcept', 'SolveMultipleChoice', 'GenerateCompleteCode',
        'DebugCode', 'GenerateTests',
      ]
      expect(helpTypes).toHaveLength(5)
    })
  })

  describe('AssistanceFallback', () => {
    it('has all 4 values', () => {
      const fallbacks: AssistanceFallback[] = [
        'None', 'ConceptualGuidance', 'SyntaxOnlyGuidance', 'AskForPolicyConfirmation',
      ]
      expect(fallbacks).toHaveLength(4)
    })
  })
})
