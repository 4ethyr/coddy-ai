// domain/reducers/sessionReducer.ts
// Pure function: ReplSession × ReplEvent → ReplSession
// Mirrors: crates/coddy-core/src/session.rs — ReplSession::apply_event()

import type { ReplSession } from '@/domain/types/session'
import type {
  ReplEvent,
  SubagentHandoffPrepared,
  SubagentLifecycleStatus,
  SubagentLifecycleUpdate,
  ToolStatus,
} from '@/domain/types/events'

const SUBAGENT_READY_SCORE = 100

export function sessionReducer(session: ReplSession, event: ReplEvent): ReplSession {
  const tag = Object.keys(event)[0] as keyof ReplEvent

  switch (tag) {
    case 'SessionStarted': {
      const { session_id } = (event as { SessionStarted: { session_id: string } }).SessionStarted
      return { ...session, id: session_id, status: 'Idle' }
    }

    case 'RunStarted': {
      const { run_id } = (event as { RunStarted: { run_id: string } }).RunStarted
      return {
        ...session,
        active_run: run_id,
        status: 'Thinking',
        streaming_text: '',
        tool_activity: [],
        subagent_activity: [],
      }
    }

    case 'VoiceListeningStarted':
      return { ...session, status: 'Listening' }

    case 'VoiceTranscriptPartial':
      return { ...session, status: 'Transcribing' }

    case 'VoiceTranscriptFinal':
      return { ...session, status: 'Thinking' }

    case 'IntentDetected':
      return { ...session, status: 'Thinking' }

    case 'SearchStarted':
      return { ...session, status: 'Thinking' }

    case 'SearchContextExtracted':
      return { ...session, status: 'BuildingContext' }

    case 'ContextItemAdded': {
      const { item } = (event as {
        ContextItemAdded: { item: ReplSession['workspace_context'][number] }
      }).ContextItemAdded
      const existingIndex = session.workspace_context.findIndex(
        (existing) => existing.id === item.id,
      )
      const workspace_context =
        existingIndex >= 0
          ? session.workspace_context.map((existing, index) =>
              index === existingIndex ? item : existing,
            )
          : [...session.workspace_context, item]

      return { ...session, status: 'BuildingContext', workspace_context }
    }

    case 'TokenDelta': {
      const { text } = (event as { TokenDelta: { run_id: string; text: string } }).TokenDelta
      return { ...session, status: 'Streaming', streaming_text: session.streaming_text + text }
    }

    case 'MessageAppended': {
      const msg = (event as { MessageAppended: { message: ReplSession['messages'][number] } })
        .MessageAppended.message
      return { ...session, messages: [...session.messages, msg], streaming_text: '' }
    }

    case 'ToolStarted': {
      const { name } = (event as { ToolStarted: { name: string } }).ToolStarted
      const tool_activity = session.tool_activity ?? []
      return {
        ...session,
        status: 'Thinking',
        tool_activity: [
          ...tool_activity,
          {
            id: `${name}-${tool_activity.length + 1}`,
            name,
            status: 'Running',
          },
        ],
      }
    }

    case 'ToolCompleted': {
      const { name, status } = (
        event as { ToolCompleted: { name: string; status: ToolStatus } }
      ).ToolCompleted
      const tool_activity = [...(session.tool_activity ?? [])]
      const runningIndex = findLastRunningToolIndex(tool_activity, name)

      if (runningIndex >= 0) {
        const activity = tool_activity[runningIndex]
        if (!activity) return { ...session, status: 'Thinking', tool_activity }
        tool_activity[runningIndex] = {
          id: activity.id,
          name: activity.name,
          status,
        }
      } else {
        tool_activity.push({
          id: `${name}-${tool_activity.length + 1}`,
          name,
          status,
        })
      }

      return { ...session, status: 'Thinking', tool_activity }
    }

    case 'SubagentRouted':
      return { ...session, status: 'Thinking' }

    case 'SubagentHandoffPrepared': {
      const { handoff } = (event as {
        SubagentHandoffPrepared: {
          handoff: SubagentHandoffPrepared
        }
      }).SubagentHandoffPrepared
      const previousActivity = session.subagent_activity ?? []
      const activityId = `${handoff.name}:${handoff.mode}`
      const existingIndex = previousActivity.findIndex(
        (item) => item.id === activityId,
      )
      const activity = subagentActivityFromHandoffPrepared(
        existingIndex >= 0 ? previousActivity[existingIndex] : undefined,
        handoff,
      )
      const subagent_activity =
        existingIndex >= 0
          ? previousActivity.map((item, index) =>
              index === existingIndex ? activity : item,
            )
          : [...previousActivity, activity]

      return { ...session, status: 'Thinking', subagent_activity }
    }

    case 'SubagentLifecycleUpdated': {
      const { update } = (event as {
        SubagentLifecycleUpdated: {
          update: SubagentLifecycleUpdate
        }
      }).SubagentLifecycleUpdated
      const previousActivity = session.subagent_activity ?? []
      const activityId = `${update.name}:${update.mode}`
      const existingIndex = previousActivity.findIndex(
        (item) => item.id === activityId,
      )
      const activity = subagentActivityFromLifecycleUpdate(
        existingIndex >= 0 ? previousActivity[existingIndex] : undefined,
        update,
      )
      const subagent_activity =
        existingIndex >= 0
          ? previousActivity.map((item, index) =>
              index === existingIndex ? activity : item,
            )
          : [...previousActivity, activity]

      return { ...session, status: 'Thinking', subagent_activity }
    }

    case 'PermissionRequested': {
      const { request } = (event as {
        PermissionRequested: { request: ReplSession['pending_permission'] }
      }).PermissionRequested
      return {
        ...session,
        status: 'AwaitingToolApproval',
        pending_permission: request,
      }
    }

    case 'PermissionReplied': {
      const { request_id } = (event as { PermissionReplied: { request_id: string } })
        .PermissionReplied
      const pending_permission =
        session.pending_permission?.id === request_id
          ? null
          : session.pending_permission

      return session.status === 'AwaitingToolApproval'
        ? {
            ...session,
            pending_permission,
            status: session.active_run ? 'Thinking' : 'Idle',
          }
        : { ...session, pending_permission }
    }

    case 'TtsStarted':
      return {
        ...session,
        voice: { ...session.voice, speaking: true },
        status: 'Speaking',
      }

    case 'TtsCompleted': {
      const newVoice = { ...session.voice, speaking: false }
      const newStatus = session.active_run ? 'Streaming' : 'Idle'
      return { ...session, voice: newVoice, status: newStatus }
    }

    case 'RunCompleted': {
      const newStatus = session.pending_permission
        ? 'AwaitingToolApproval'
        : session.voice.speaking
          ? 'Speaking'
          : 'Idle'
      return { ...session, active_run: null, status: newStatus, streaming_text: '' }
    }

    case 'Error':
      return { ...session, status: 'Error' }

    case 'OverlayShown': {
      const { mode } = (event as { OverlayShown: { mode: ReplSession['mode'] } }).OverlayShown
      return { ...session, mode }
    }

    case 'PolicyEvaluated': {
      const { policy, allowed } = (event as { PolicyEvaluated: { policy: string; allowed: boolean } })
        .PolicyEvaluated
      return {
        ...session,
        policy: policy as ReplSession['policy'],
        status: !allowed && policy === 'UnknownAssessment'
          ? 'AwaitingConfirmation'
          : session.status,
      }
    }

    case 'ConfirmationDismissed':
      return session.status === 'AwaitingConfirmation'
        ? { ...session, status: 'Idle' }
        : session

    case 'ModelSelected': {
      const { model, role } = (event as { ModelSelected: {
        model: ReplSession['selected_model']
        role: string
      } }).ModelSelected

      if (role !== 'Chat') return session

      return { ...session, selected_model: model }
    }

    // Events that the frontend observes but does not mutate state for:
    case 'ShortcutTriggered':
    case 'ScreenCaptured':
    case 'OcrCompleted':
    case 'TtsQueued':
      return session

    default:
      return session
  }
}

function findLastRunningToolIndex(
  activities: ReplSession['tool_activity'],
  name: string,
): number {
  for (let index = activities.length - 1; index >= 0; index--) {
    const activity = activities[index]
    if (activity?.name === name && activity.status === 'Running') {
      return index
    }
  }
  return -1
}

function subagentActivityFromLifecycleUpdate(
  previous: ReplSession['subagent_activity'][number] | undefined,
  update: SubagentLifecycleUpdate,
): ReplSession['subagent_activity'][number] {
  const { status, reason } = normalizeSubagentLifecycleTransition(previous?.status, update)

  return {
    id: `${update.name}:${update.mode}`,
    name: update.name,
    mode: update.mode,
    status,
    readiness_score: update.readiness_score,
    required_output_fields: previous?.required_output_fields ?? [],
    output_additional_properties_allowed:
      previous?.output_additional_properties_allowed ?? true,
    reason,
  }
}

function subagentActivityFromHandoffPrepared(
  previous: ReplSession['subagent_activity'][number] | undefined,
  handoff: SubagentHandoffPrepared,
): ReplSession['subagent_activity'][number] {
  const reason = handoffReadinessReason(handoff)
  return {
    id: `${handoff.name}:${handoff.mode}`,
    name: handoff.name,
    mode: handoff.mode,
    status: reason ? 'Blocked' : previous?.status ?? 'Prepared',
    readiness_score: handoff.readiness_score,
    required_output_fields: handoff.required_output_fields,
    output_additional_properties_allowed:
      handoff.output_additional_properties_allowed,
    reason: reason ?? previous?.reason ?? null,
  }
}

function normalizeSubagentLifecycleTransition(
  previous: SubagentLifecycleStatus | undefined,
  update: SubagentLifecycleUpdate,
): { status: SubagentLifecycleStatus; reason: string | null } {
  const readinessReason = readinessBlockReason(update)
  if (readinessReason) {
    return { status: 'Blocked', reason: readinessReason }
  }

  if (!isAllowedSubagentTransition(previous, update.status)) {
    return {
      status: 'Blocked',
      reason: `invalid subagent lifecycle transition: ${previous ?? 'None'} -> ${update.status}`,
    }
  }

  return { status: update.status, reason: update.reason }
}

function readinessBlockReason(update: SubagentLifecycleUpdate): string | null {
  if (!requiresReadyHandoff(update.status)) return null

  const reasons: string[] = []
  if (update.readiness_score < SUBAGENT_READY_SCORE) {
    reasons.push(`readiness score ${update.readiness_score} is below execution threshold`)
  }
  if (update.reason) {
    reasons.push(update.reason)
  }

  return reasons.length > 0 ? reasons.join('; ') : null
}

function handoffReadinessReason(handoff: SubagentHandoffPrepared): string | null {
  const reasons: string[] = []
  if (handoff.readiness_score < SUBAGENT_READY_SCORE) {
    reasons.push(`readiness score ${handoff.readiness_score} is below execution threshold`)
  }
  reasons.push(...handoff.readiness_issues)

  return reasons.length > 0 ? reasons.join('; ') : null
}

function requiresReadyHandoff(status: SubagentLifecycleStatus): boolean {
  return (
    status === 'Prepared'
    || status === 'Approved'
    || status === 'Running'
    || status === 'Completed'
  )
}

function isAllowedSubagentTransition(
  previous: SubagentLifecycleStatus | undefined,
  next: SubagentLifecycleStatus,
): boolean {
  if (!previous) return next === 'Prepared' || next === 'Blocked'
  if (previous === next) return true

  if (previous === 'Prepared') return next === 'Approved' || next === 'Blocked'
  if (previous === 'Approved') return next === 'Running' || next === 'Blocked'
  if (previous === 'Running') {
    return next === 'Completed' || next === 'Failed' || next === 'Blocked'
  }

  return false
}
