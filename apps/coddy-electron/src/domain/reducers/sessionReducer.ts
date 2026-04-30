// domain/reducers/sessionReducer.ts
// Pure function: ReplSession × ReplEvent → ReplSession
// Mirrors: crates/coddy-core/src/session.rs — ReplSession::apply_event()

import type { ReplSession } from '@/domain/types/session'
import type { ReplEvent, ToolStatus } from '@/domain/types/events'

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
