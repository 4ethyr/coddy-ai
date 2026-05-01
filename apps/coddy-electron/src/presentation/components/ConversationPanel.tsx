// ConversationPanel: chat messages + input bar for DesktopApp.

import { useRef, useEffect } from 'react'
import type { ReplSession } from '@/domain'
import {
  MarkdownContent,
  MessageBubble,
} from '@/presentation/components/MessageBubble'
import { InputBar } from '@/presentation/components/InputBar'
import { ToolApprovalPanel } from '@/presentation/components/ToolApprovalPanel'
import { SelectionCopyRegion } from '@/presentation/components/SelectionCopyRegion'
import {
  ThinkingIndicator,
  type ThinkingAnimation,
} from '@/presentation/components/ThinkingIndicator'
import { Icon } from '@/presentation/components/Icon'
import type { PermissionReply } from '@/domain'

interface Props {
  session: ReplSession
  onSend: (text: string) => void
  onPermissionReply: (requestId: string, reply: PermissionReply) => void
  thinkingAnimation?: ThinkingAnimation
}

export function ConversationPanel({
  session,
  onSend,
  onPermissionReply,
  thinkingAnimation = 'scan',
}: Props) {
  const messagesEndRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView?.({ behavior: 'smooth' })
  }, [session.messages.length, session.streaming_text])

  return (
    <div className="relative flex h-full flex-1 flex-col overflow-hidden">
      <SelectionCopyRegion className="desktop-canvas flex-1 overflow-y-auto px-4 py-6 sm:px-8">
        <div className="mx-auto flex max-w-5xl flex-col gap-7 pb-28">
          <div className="flex justify-center">
            <div className="rounded border border-white/5 bg-surface-container/40 px-4 py-2 text-center backdrop-blur-md">
              <p className="font-mono text-xs uppercase tracking-[0.18em] text-on-surface-variant/45">
                session_initialized // awaiting command
              </p>
            </div>
          </div>

          <PlanOfAttack session={session} />

          {session.messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}

          {session.status === 'Thinking' && !session.streaming_text && (
            <ThinkingIndicator animation={thinkingAnimation} />
          )}

          {session.streaming_text && (
            <div className="flex items-start gap-4">
              <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary bg-primary/10 text-primary">
                <Icon name="bot" className="h-4 w-4" />
              </div>
              <div className="desktop-glass-panel max-w-3xl flex-1 rounded-lg px-5 py-4">
                <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
                  coddy_agent
                </div>
                <MarkdownContent text={session.streaming_text} />
                <span className="streaming-cursor mt-2 inline-block" />
                <p className="mt-3 text-[11px] text-on-surface-muted">
                  Pressione (Esc) para parar.
                </p>
              </div>
            </div>
          )}

          {session.pending_permission && (
            <ToolApprovalPanel
              request={session.pending_permission}
              onReply={onPermissionReply}
            />
          )}

          <div ref={messagesEndRef} />
        </div>
      </SelectionCopyRegion>

      <div className="pointer-events-none absolute bottom-5 left-0 right-0 z-20 flex justify-center px-4">
        <div className="pointer-events-auto w-full max-w-3xl rounded-full border border-white/15 bg-surface-container/90 p-2 backdrop-blur-2xl">
          <InputBar
            onSend={onSend}
            disabled={
              session.status === 'Streaming'
              || session.status === 'Thinking'
              || session.status === 'AwaitingToolApproval'
            }
            placeholder={
              session.status === 'Thinking'
                ? 'Thinking...'
                : session.status === 'AwaitingToolApproval'
                  ? 'Tool approval required'
                : 'Instruct Coddy agent...'
            }
          />
        </div>
      </div>
    </div>
  )
}

function PlanOfAttack({ session }: { session: ReplSession }) {
  const toolActivity = session.tool_activity ?? []
  const subagentActivity = session.subagent_activity ?? []
  const hasToolActivity = toolActivity.length > 0
  const hasSubagentActivity = subagentActivity.length > 0

  return (
    <section className="desktop-glass-panel overflow-hidden rounded-xl">
      <div className="border-b border-white/5 bg-gradient-to-br from-surface-container-high/80 to-transparent p-5">
        <h2 className="mb-4 flex items-center gap-2 font-display text-[11px] uppercase tracking-[0.2em] text-primary">
          <Icon name="sensors" className="h-4 w-4" />
          Agent activity
        </h2>
        <div className="flex flex-col gap-0 pl-1">
          <TaskStep
            label={session.active_run ? 'Run is active' : 'Waiting for run'}
            state={session.active_run ? 'active' : 'pending'}
          />
          {hasToolActivity ? (
            toolActivity.map((activity) => (
              <TaskStep
                key={activity.id}
                label={`${activity.name} // ${activity.status}`}
                state={activity.status === 'Running' ? 'active' : 'done'}
              />
            ))
          ) : (
            <TaskStep label="No tools used in this run" state="pending" />
          )}
          {hasSubagentActivity ? (
            subagentActivity.map((activity) => (
              <TaskStep
                key={activity.id}
                label={`subagent.${activity.name} // ${activity.status} // readiness=${activity.readiness_score}${subagentOutputSuffix(activity.required_output_fields, activity.output_additional_properties_allowed)}`}
                state={subagentState(activity.status)}
              />
            ))
          ) : (
            <TaskStep label="No subagent handoff prepared" state="pending" />
          )}
        </div>
      </div>
    </section>
  )
}

function subagentOutputSuffix(
  fields: string[],
  additionalPropertiesAllowed: boolean,
): string {
  if (fields.length === 0) return ''
  const visibleFields = fields.slice(0, 3).join(', ')
  const remaining = fields.length > 3 ? ` +${fields.length - 3}` : ''
  const strictness = additionalPropertiesAllowed ? 'open' : 'strict'
  return ` // output=${visibleFields}${remaining} // ${strictness}`
}

function subagentState(
  status: ReplSession['subagent_activity'][number]['status'],
): 'done' | 'active' | 'pending' | 'blocked' {
  if (status === 'Running') return 'active'
  if (status === 'Blocked' || status === 'Failed') return 'blocked'
  if (status === 'Prepared' || status === 'Approved' || status === 'Completed') {
    return 'done'
  }
  return 'pending'
}

function TaskStep({
  label,
  state,
}: {
  label: string
  state: 'done' | 'active' | 'pending' | 'blocked'
}) {
  return (
    <div className="relative flex gap-4 pb-3 last:pb-0">
      {state !== 'pending' && (
        <div className="absolute left-[5px] top-4 h-full w-px bg-outline-variant/50" />
      )}
      <span
        className={`z-10 mt-1.5 flex h-3 w-3 shrink-0 items-center justify-center rounded-full border ${
          state === 'active'
            ? 'border-primary bg-surface-dim shadow-[0_0_8px_rgba(0,219,233,0.6)]'
            : state === 'blocked'
              ? 'border-red-300/70 bg-red-400/10'
            : 'border-outline-variant bg-surface-dim'
        }`}
      >
        {state !== 'pending' && (
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              state === 'active'
                ? 'bg-primary'
                : state === 'blocked'
                  ? 'bg-red-300'
                  : 'bg-outline'
            }`}
          />
        )}
      </span>
      <p
        className={`font-mono text-sm ${
          state === 'active'
            ? 'font-bold text-primary'
            : state === 'blocked'
              ? 'text-red-300'
            : state === 'done'
              ? 'text-on-surface-variant/60 line-through'
              : 'text-on-surface-variant/45'
        }`}
      >
        {label}
      </p>
    </div>
  )
}
