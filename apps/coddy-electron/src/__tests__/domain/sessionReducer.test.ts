import { describe, it, expect } from 'vitest'
import { createInitialSession, type ReplSession } from '@/domain/types/session'
import type { ReplEvent, ReplMode } from '@/domain/types/events'
import { sessionReducer } from '@/domain/reducers/sessionReducer'

// Helper: create a default test session
function testSession(overrides?: Partial<ReplSession>): ReplSession {
  return {
    id: '',
    mode: 'FloatingTerminal' as ReplMode,
    status: 'Idle',
    policy: 'UnknownAssessment',
    selected_model: { provider: 'ollama', name: 'test-model' },
    voice: { enabled: true, speaking: false, muted: false },
    screen_context: null,
    workspace_context: [],
    messages: [],
    active_run: null,
    pending_permission: null,
    agent_run: null,
    tool_activity: [],
    subagent_activity: [],
    streaming_text: '',
    ...overrides,
  }
}

// Helper: apply multiple events to a session
function reduceEvents(session: ReplSession, events: ReplEvent[]): ReplSession {
  return events.reduce((s, e) => sessionReducer(s, e), session)
}

describe('sessionReducer', () => {
  describe('SessionStarted', () => {
    it('sets session id and transitions to Idle', () => {
      const session = createInitialSession('FloatingTerminal', {
        provider: 'ollama',
        name: 'gemma4',
      })

      const event: ReplEvent = {
        SessionStarted: { session_id: '550e8400-e29b-41d4-a716-446655440000' },
      }

      const result = sessionReducer(session, event)

      expect(result.id).toBe('550e8400-e29b-41d4-a716-446655440000')
      expect(result.status).toBe('Idle')
      expect(result.mode).toBe('FloatingTerminal')
    })
  })

  describe('RunStarted', () => {
    it('sets active_run, clears run-local activity, transitions to Thinking', () => {
      const session = testSession({
        streaming_text: 'previous tokens',
        agent_run: {
          run_id: 'previous-run',
          summary: {
            goal: 'previous task',
            last_phase: 'Completed',
            completed_steps: 3,
            stop_reason: null,
            failure_code: null,
            failure_message: null,
            recoverable_failure: false,
          },
        },
        tool_activity: [
          { id: 'filesystem.read_file-1', name: 'filesystem.read_file', status: 'Succeeded' },
        ],
        subagent_activity: [
          {
            id: 'eval-runner:evaluation',
            name: 'eval-runner',
            mode: 'evaluation',
            status: 'Prepared',
            readiness_score: 100,
            required_output_fields: ['score', 'passed'],
            output_additional_properties_allowed: false,
            reason: null,
          },
        ],
      })
      const event: ReplEvent = {
        RunStarted: { run_id: 'run-001' },
      }

      const result = sessionReducer(session, event)

      expect(result.active_run).toBe('run-001')
      expect(result.status).toBe('Thinking')
      expect(result.agent_run).toBeNull()
      expect(result.streaming_text).toBe('')
      expect(result.tool_activity).toEqual([])
      expect(result.subagent_activity).toEqual([])
    })
  })

  describe('AgentRunUpdated', () => {
    it('records the latest observable agent run summary', () => {
      const session = testSession({ active_run: 'run-001', status: 'Thinking' })
      const event: ReplEvent = {
        AgentRunUpdated: {
          run_id: 'run-001',
          summary: {
            goal: 'list files',
            last_phase: 'Completed',
            completed_steps: 3,
            stop_reason: null,
            failure_code: null,
            failure_message: null,
            recoverable_failure: false,
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.agent_run).toEqual({
        run_id: 'run-001',
        summary: {
          goal: 'list files',
          last_phase: 'Completed',
          completed_steps: 3,
          stop_reason: null,
          failure_code: null,
          failure_message: null,
          recoverable_failure: false,
        },
      })
      expect(result.status).toBe('Thinking')
    })
  })

  describe('VoiceListeningStarted', () => {
    it('transitions to Listening', () => {
      const session = testSession()
      const event: ReplEvent = { VoiceListeningStarted: {} }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Listening')
    })
  })

  describe('VoiceTranscriptPartial', () => {
    it('transitions to Transcribing', () => {
      const session = testSession()
      const event: ReplEvent = {
        VoiceTranscriptPartial: { text: 'open terminal' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Transcribing')
    })
  })

  describe('VoiceTranscriptFinal', () => {
    it('transitions to Thinking', () => {
      const session = testSession()
      const event: ReplEvent = {
        VoiceTranscriptFinal: { text: 'open terminal' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
    })
  })

  describe('SearchStarted', () => {
    it('transitions to Thinking', () => {
      const session = testSession({ status: 'Listening' })
      const event: ReplEvent = {
        SearchStarted: { query: 'Rust docs', provider: 'google' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
    })
  })

  describe('SearchContextExtracted', () => {
    it('transitions to BuildingContext', () => {
      const session = testSession()
      const event: ReplEvent = {
        SearchContextExtracted: {
          provider: 'google',
          organic_results: 10,
          ai_overview_present: true,
        },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('BuildingContext')
    })
  })

  describe('ContextItemAdded', () => {
    it('adds workspace context and transitions to BuildingContext', () => {
      const session = testSession()
      const event: ReplEvent = {
        ContextItemAdded: {
          item: {
            id: 'tool:filesystem.read_file:src/main.rs',
            label: 'filesystem.read_file: src/main.rs',
            sensitive: false,
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('BuildingContext')
      expect(result.workspace_context).toEqual([
        {
          id: 'tool:filesystem.read_file:src/main.rs',
          label: 'filesystem.read_file: src/main.rs',
          sensitive: false,
        },
      ])
    })

    it('upserts existing workspace context by id', () => {
      const session = testSession({
        workspace_context: [
          {
            id: 'tool:filesystem.read_file:.env',
            label: 'filesystem.read_file: .env',
            sensitive: false,
          },
        ],
      })
      const event: ReplEvent = {
        ContextItemAdded: {
          item: {
            id: 'tool:filesystem.read_file:.env',
            label: 'filesystem.read_file: .env',
            sensitive: true,
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.workspace_context).toHaveLength(1)
      expect(result.workspace_context[0]?.sensitive).toBe(true)
    })
  })

  describe('TokenDelta', () => {
    it('transitions to Streaming', () => {
      const session = testSession({ active_run: 'run-001' })
      const event: ReplEvent = {
        TokenDelta: { run_id: 'run-001', text: 'Hello' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Streaming')
    })

    it('accumulates text into streaming_text', () => {
      const session = testSession({ active_run: 'run-001' })
      const events: ReplEvent[] = [
        { TokenDelta: { run_id: 'run-001', text: 'Hello' } },
        { TokenDelta: { run_id: 'run-001', text: ' world' } },
      ]

      const result = reduceEvents(session, events)

      expect(result.streaming_text).toBe('Hello world')
    })
  })

  describe('MessageAppended', () => {
    it('appends message to messages array and clears streaming_text', () => {
      const session = testSession({ streaming_text: 'in progress tokens' })
      const event: ReplEvent = {
        MessageAppended: {
          message: { id: 'msg-1', role: 'user', text: 'hello' },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.messages).toHaveLength(1)
      expect(result.messages[0]?.text).toBe('hello')
      expect(result.messages[0]?.role).toBe('user')
      expect(result.streaming_text).toBe('')
    })

    it('appends multiple messages in sequence', () => {
      const session = testSession()
      const events: ReplEvent[] = [
        { MessageAppended: { message: { id: 'msg-1', role: 'user', text: 'hi' } } },
        { MessageAppended: { message: { id: 'msg-2', role: 'assistant', text: 'hello!' } } },
      ]

      const result = reduceEvents(session, events)

      expect(result.messages).toHaveLength(2)
      expect(result.messages[0]?.role).toBe('user')
      expect(result.messages[1]?.role).toBe('assistant')
    })
  })

  describe('TtsStarted', () => {
    it('marks voice.speaking = true and transitions to Speaking', () => {
      const session = testSession({ status: 'Streaming' })
      const event: ReplEvent = { TtsStarted: {} }

      const result = sessionReducer(session, event)

      expect(result.voice.speaking).toBe(true)
      expect(result.status).toBe('Speaking')
    })
  })

  describe('TtsCompleted', () => {
    it('marks voice.speaking = false', () => {
      const session = testSession({
        status: 'Speaking',
        voice: { enabled: true, speaking: true, muted: false },
      })

      const event: ReplEvent = { TtsCompleted: {} }

      const result = sessionReducer(session, event)

      expect(result.voice.speaking).toBe(false)
    })

    it('reverts to Idle when no active run', () => {
      const session = testSession({
        status: 'Speaking',
        active_run: null,
        voice: { enabled: true, speaking: true, muted: false },
      })

      const event: ReplEvent = { TtsCompleted: {} }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Idle')
    })

    it('reverts to Streaming when active_run exists', () => {
      const session = testSession({
        status: 'Speaking',
        active_run: 'run-001',
        voice: { enabled: true, speaking: true, muted: false },
      })

      const event: ReplEvent = { TtsCompleted: {} }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Streaming')
    })
  })

  describe('RunCompleted', () => {
    it('clears active_run, streaming_text, and reverts to Idle when not speaking', () => {
      const session = testSession({
        status: 'Streaming',
        active_run: 'run-001',
        streaming_text: 'finished text',
      })

      const event: ReplEvent = { RunCompleted: { run_id: 'run-001' } }

      const result = sessionReducer(session, event)

      expect(result.active_run).toBeNull()
      expect(result.status).toBe('Idle')
      expect(result.streaming_text).toBe('')
    })

    it('stays in Speaking if voice is speaking, but clears streaming_text', () => {
      const session = testSession({
        status: 'Speaking',
        active_run: 'run-001',
        streaming_text: 'final tokens',
        voice: { enabled: true, speaking: true, muted: false },
      })

      const event: ReplEvent = { RunCompleted: { run_id: 'run-001' } }

      const result = sessionReducer(session, event)

      expect(result.active_run).toBeNull()
      expect(result.status).toBe('Speaking')
      expect(result.streaming_text).toBe('')
    })
  })

  describe('Error', () => {
    it('transitions to Error status with error info preserved in messages', () => {
      const session = testSession({ active_run: 'run-001' })
      const event: ReplEvent = {
        Error: { code: 'E_PARSE', message: 'Failed to parse response' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Error')
      // sessionReducer preserves the session as-is except status
      expect(result.active_run).toBe('run-001')
    })
  })

  describe('OverlayShown', () => {
    it('updates mode', () => {
      const session = testSession({ mode: 'FloatingTerminal' as ReplMode })
      const event: ReplEvent = {
        OverlayShown: { mode: 'DesktopApp' as ReplMode },
      }

      const result = sessionReducer(session, event)

      expect(result.mode).toBe('DesktopApp')
    })
  })

  describe('PolicyEvaluated', () => {
    it('updates policy', () => {
      const session = testSession()
      const event: ReplEvent = {
        PolicyEvaluated: { policy: 'SyntaxOnly', allowed: false },
      }

      const result = sessionReducer(session, event)

      expect(result.policy).toBe('SyntaxOnly')
    })

    it('waits for confirmation when assessment policy is unknown', () => {
      const session = testSession({ status: 'Thinking' })
      const event: ReplEvent = {
        PolicyEvaluated: { policy: 'UnknownAssessment', allowed: false },
      }

      const result = sessionReducer(session, event)

      expect(result.policy).toBe('UnknownAssessment')
      expect(result.status).toBe('AwaitingConfirmation')
    })

    it('returns to Idle when confirmation is dismissed', () => {
      const session = testSession({ status: 'AwaitingConfirmation' })
      const event: ReplEvent = { ConfirmationDismissed: {} }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Idle')
    })
  })

  describe('ModelSelected', () => {
    it('updates selected_model for chat role', () => {
      const session = testSession()
      const event: ReplEvent = {
        ModelSelected: {
          model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
          role: 'Chat',
        },
      }

      const result = sessionReducer(session, event)

      expect(result.selected_model).toEqual({
        provider: 'ollama',
        name: 'qwen2.5:0.5b',
      })
    })

    it('does not replace selected chat model for OCR role', () => {
      const session = testSession({
        selected_model: { provider: 'ollama', name: 'gemma4-e2b' },
      })
      const event: ReplEvent = {
        ModelSelected: {
          model: { provider: 'ollama', name: 'glm-ocr' },
          role: 'Ocr',
        },
      }

      const result = sessionReducer(session, event)

      expect(result.selected_model).toEqual({
        provider: 'ollama',
        name: 'gemma4-e2b',
      })
    })
  })

  describe('Intents and tool events', () => {
    it('IntentDetected transitions to Thinking', () => {
      const session = testSession({ status: 'Listening' })
      const event: ReplEvent = {
        IntentDetected: { intent: 'OpenApplication', confidence: 0.95 },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
    })

    it('ToolStarted records running tool activity', () => {
      const session = testSession({ status: 'Thinking' })
      const event: ReplEvent = { ToolStarted: { name: 'search_web' } }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
      expect(result.tool_activity).toEqual([
        { id: 'search_web-1', name: 'search_web', status: 'Running' },
      ])
    })

    it('ToolCompleted updates the last running tool activity', () => {
      const session = testSession({
        status: 'Thinking',
        tool_activity: [
          { id: 'search_web-1', name: 'search_web', status: 'Running' },
        ],
      })
      const event: ReplEvent = {
        ToolCompleted: { name: 'search_web', status: 'Denied' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
      expect(result.tool_activity).toEqual([
        { id: 'search_web-1', name: 'search_web', status: 'Denied' },
      ])
    })

    it('ToolCompleted records orphan completion events', () => {
      const session = testSession({ status: 'Thinking' })
      const event: ReplEvent = {
        ToolCompleted: { name: 'filesystem.list_files', status: 'Succeeded' },
      }

      const result = sessionReducer(session, event)

      expect(result.tool_activity).toEqual([
        {
          id: 'filesystem.list_files-1',
          name: 'filesystem.list_files',
          status: 'Succeeded',
        },
      ])
    })

    it('SubagentRouted keeps thinking state without mutating tool activity', () => {
      const session = testSession({
        status: 'Thinking',
        tool_activity: [
          { id: 'subagent.route-1', name: 'subagent.route', status: 'Succeeded' },
        ],
      })
      const event: ReplEvent = {
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
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
      expect(result.tool_activity).toEqual(session.tool_activity)
    })

    it('SubagentHandoffPrepared keeps thinking state without mutating tool activity', () => {
      const session = testSession({
        status: 'Thinking',
        tool_activity: [
          { id: 'subagent.prepare-1', name: 'subagent.prepare', status: 'Succeeded' },
        ],
      })
      const event: ReplEvent = {
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
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
      expect(result.tool_activity).toEqual(session.tool_activity)
      expect(result.subagent_activity).toEqual([
        {
          id: 'eval-runner:evaluation',
          name: 'eval-runner',
          mode: 'evaluation',
          status: 'Prepared',
          readiness_score: 100,
          required_output_fields: ['score', 'passed'],
          output_additional_properties_allowed: false,
          reason: null,
        },
      ])
    })

    it('SubagentLifecycleUpdated upserts subagent activity without mutating tool activity', () => {
      const session = testSession({
        status: 'Thinking',
        tool_activity: [
          { id: 'subagent.prepare-1', name: 'subagent.prepare', status: 'Succeeded' },
        ],
      })
      const event: ReplEvent = {
        SubagentLifecycleUpdated: {
          update: {
            name: 'eval-runner',
            mode: 'evaluation',
            status: 'Prepared',
            readiness_score: 100,
            reason: null,
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
      expect(result.tool_activity).toEqual(session.tool_activity)
      expect(result.subagent_activity).toEqual([
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

    it('SubagentLifecycleUpdated replaces existing subagent activity by role and mode', () => {
      const session = testSession({
        status: 'Thinking',
        subagent_activity: [
          {
            id: 'coder:workspace-write',
            name: 'coder',
            mode: 'workspace-write',
            status: 'Prepared',
            readiness_score: 100,
            required_output_fields: ['changedFiles', 'summary'],
            output_additional_properties_allowed: false,
            reason: null,
          },
        ],
      })
      const event: ReplEvent = {
        SubagentLifecycleUpdated: {
          update: {
            name: 'coder',
            mode: 'workspace-write',
            status: 'Blocked',
            readiness_score: 80,
            reason: 'workspace-write handoff must include preview edit capability',
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.subagent_activity).toEqual([
        {
          id: 'coder:workspace-write',
          name: 'coder',
          mode: 'workspace-write',
          status: 'Blocked',
          readiness_score: 80,
          required_output_fields: ['changedFiles', 'summary'],
          output_additional_properties_allowed: false,
          reason: 'workspace-write handoff must include preview edit capability',
        },
      ])
    })

    it('SubagentLifecycleUpdated blocks Running without prior approval', () => {
      const session = testSession({ status: 'Thinking' })
      const event: ReplEvent = {
        SubagentLifecycleUpdated: {
          update: {
            name: 'coder',
            mode: 'workspace-write',
            status: 'Running',
            readiness_score: 100,
            reason: null,
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.subagent_activity).toEqual([
        {
          id: 'coder:workspace-write',
          name: 'coder',
          mode: 'workspace-write',
          status: 'Blocked',
          readiness_score: 100,
          required_output_fields: [],
          output_additional_properties_allowed: true,
          reason: 'invalid subagent lifecycle transition: None -> Running',
        },
      ])
    })

    it('SubagentLifecycleUpdated blocks executable statuses below readiness threshold', () => {
      const session = testSession({ status: 'Thinking' })
      const event: ReplEvent = {
        SubagentLifecycleUpdated: {
          update: {
            name: 'coder',
            mode: 'workspace-write',
            status: 'Prepared',
            readiness_score: 80,
            reason: 'missing preview edit capability',
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.subagent_activity[0]).toMatchObject({
        status: 'Blocked',
        readiness_score: 80,
        reason: 'readiness score 80 is below execution threshold; missing preview edit capability',
      })
    })

    it('SubagentLifecycleUpdated allows prepared approved running completed sequence', () => {
      const events: ReplEvent[] = ['Prepared', 'Approved', 'Running', 'Completed'].map(
        (status) => ({
          SubagentLifecycleUpdated: {
            update: {
              name: 'eval-runner',
              mode: 'evaluation',
              status: status as 'Prepared' | 'Approved' | 'Running' | 'Completed',
              readiness_score: 100,
              reason: null,
            },
          },
        }),
      )

      const result = reduceEvents(testSession({ status: 'Thinking' }), events)

      expect(result.subagent_activity).toEqual([
        {
          id: 'eval-runner:evaluation',
          name: 'eval-runner',
          mode: 'evaluation',
          status: 'Completed',
          readiness_score: 100,
          required_output_fields: [],
          output_additional_properties_allowed: true,
          reason: null,
        },
      ])
    })

    it('PermissionRequested transitions to AwaitingToolApproval and stores the request', () => {
      const session = testSession({ status: 'Thinking', active_run: 'run-001' })
      const event: ReplEvent = {
        PermissionRequested: {
          request: {
            id: 'perm-1',
            session_id: 'session-1',
            run_id: 'run-001',
            tool_call_id: 'call-1',
            tool_name: 'filesystem.apply_edit',
            permission: 'WriteWorkspace',
            patterns: ['src/App.tsx'],
            risk_level: 'High',
            metadata: { path: 'src/App.tsx' },
            requested_at_unix_ms: 1775000000000,
          },
        },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('AwaitingToolApproval')
      expect(result.pending_permission?.id).toBe('perm-1')
    })

    it('RunCompleted preserves AwaitingToolApproval while permission is pending', () => {
      const session = testSession({
        status: 'AwaitingToolApproval',
        active_run: 'run-001',
        pending_permission: {
          id: 'perm-1',
          session_id: 'session-1',
          run_id: 'run-001',
          tool_call_id: 'call-1',
          tool_name: 'filesystem.apply_edit',
          permission: 'WriteWorkspace',
          patterns: ['src/App.tsx'],
          risk_level: 'High',
          metadata: { path: 'src/App.tsx' },
          requested_at_unix_ms: 1775000000000,
        },
        streaming_text: 'finished',
      })
      const event: ReplEvent = {
        RunCompleted: { run_id: 'run-001' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('AwaitingToolApproval')
      expect(result.active_run).toBeNull()
      expect(result.pending_permission?.id).toBe('perm-1')
      expect(result.streaming_text).toBe('')
    })

    it('PermissionReplied returns to Thinking when an active run exists', () => {
      const session = testSession({
        status: 'AwaitingToolApproval',
        active_run: 'run-001',
        pending_permission: {
          id: 'perm-1',
          session_id: 'session-1',
          run_id: 'run-001',
          tool_call_id: null,
          tool_name: 'filesystem.apply_edit',
          permission: 'WriteWorkspace',
          patterns: ['src/App.tsx'],
          risk_level: 'High',
          metadata: { path: 'src/App.tsx' },
          requested_at_unix_ms: 1775000000000,
        },
      })
      const event: ReplEvent = {
        PermissionReplied: { request_id: 'perm-1', reply: 'Once' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Thinking')
      expect(result.pending_permission).toBeNull()
    })

    it('PermissionReplied returns to Idle when no active run exists', () => {
      const session = testSession({
        status: 'AwaitingToolApproval',
        active_run: null,
        pending_permission: {
          id: 'perm-1',
          session_id: 'session-1',
          run_id: 'run-001',
          tool_call_id: null,
          tool_name: 'filesystem.apply_edit',
          permission: 'WriteWorkspace',
          patterns: ['src/App.tsx'],
          risk_level: 'High',
          metadata: { path: 'src/App.tsx' },
          requested_at_unix_ms: 1775000000000,
        },
      })
      const event: ReplEvent = {
        PermissionReplied: { request_id: 'perm-1', reply: 'Reject' },
      }

      const result = sessionReducer(session, event)

      expect(result.status).toBe('Idle')
      expect(result.pending_permission).toBeNull()
    })
  })

  describe('immutability', () => {
    it('returns a new object (does not mutate original)', () => {
      const session = testSession()
      const event: ReplEvent = {
        RunStarted: { run_id: 'run-001' },
      }

      const result = sessionReducer(session, event)

      expect(result).not.toBe(session)
      expect(session.status).toBe('Idle') // original unchanged
      expect(session.active_run).toBeNull() // original unchanged
    })
  })
})
