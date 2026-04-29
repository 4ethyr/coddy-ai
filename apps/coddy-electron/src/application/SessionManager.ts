// application/SessionManager.ts
// Use case: initializes a REPL session from snapshot and manages its lifecycle.

import type { ReplIpcClient, ReplToolCatalogItem } from '@/domain'
import type { ReplSession, ReplEventEnvelope } from '@/domain'
import { sessionReducer, createInitialSession } from '@/domain'

export interface SessionState {
  session: ReplSession
  lastSequence: number
  toolCatalog: ReplToolCatalogItem[]
}

/**
 * Fetches the snapshot from the backend and builds the initial
 * session state. On success, returns the state so the UI can render.
 */
export async function initializeSession(
  client: ReplIpcClient,
): Promise<SessionState> {
  const [snapshot, toolCatalog] = await Promise.all([
    client.getSnapshot(),
    client.getToolCatalog(),
  ])

  // Cast the raw JSON-serialized session (string enums) to the typed ReplSession.
  // The field values are validated by the Rust backend at serialization time.
  const session = snapshot.session as unknown as ReplSession

  // Ensure frontend-only fields have defaults
  if (!session.streaming_text) {
    session.streaming_text = ''
  }
  if (!session.tool_activity) {
    session.tool_activity = []
  }

  return {
    session,
    lastSequence: snapshot.last_sequence,
    toolCatalog,
  }
}

/**
 * Creates a fresh local session (when no daemon is available yet).
 */
export function createLocalSession(): SessionState {
  return {
    session: createInitialSession('FloatingTerminal', {
      provider: 'ollama',
      name: 'gemma4:e2b',
    }),
    lastSequence: 0,
    toolCatalog: [],
  }
}

/**
 * Applies a batch of events to the current session state, returning
 * the new state and the updated last sequence.
 */
export function applyEvents(
  state: SessionState,
  events: ReplEventEnvelope[],
  newLastSequence: number,
): SessionState {
  let session = state.session
  for (const envelope of events) {
    session = sessionReducer(session, envelope.event)
  }
  return {
    session,
    lastSequence: newLastSequence,
    toolCatalog: state.toolCatalog,
  }
}
