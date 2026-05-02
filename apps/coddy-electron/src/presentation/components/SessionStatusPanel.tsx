import type { ReplSession } from '@/domain'
import { Icon } from './Icon'

interface Props {
  session: ReplSession
  workspacePath?: string | null
  toolCount?: number
  onClose: () => void
}

export function SessionStatusPanel({
  session,
  workspacePath = null,
  toolCount = 0,
  onClose,
}: Props) {
  const lines = [
    ['status', session.status],
    ['mode', session.mode],
    ['model', `${session.selected_model.provider}/${session.selected_model.name}`],
    ['workspace', workspacePath?.trim() || 'not selected'],
    ['active_run', session.active_run ?? 'none'],
    ['messages', String(session.messages.length)],
    ['tools', String(toolCount)],
    ['subagents', String(session.subagent_activity.length)],
  ] as const

  return (
    <section
      className="rounded-lg border border-primary/20 bg-surface-container/55 p-4 backdrop-blur-md"
      aria-label="Session status"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon name="sensors" className="h-4 w-4 text-primary" />
          <h2 className="font-mono text-[11px] uppercase tracking-[0.2em] text-primary">
            repl.status
          </h2>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-on-surface-variant transition-colors hover:text-primary"
          aria-label="Close status"
          title="Close status"
        >
          <Icon name="close" className="h-4 w-4" />
        </button>
      </div>

      <div className="grid gap-2 font-mono text-xs text-on-surface-variant sm:grid-cols-2">
        {lines.map(([label, value]) => (
          <div
            key={label}
            className="min-w-0 rounded border border-white/10 bg-surface-container-high/35 px-3 py-2"
          >
            <span className="break-words">{label}={value}</span>
          </div>
        ))}
      </div>
    </section>
  )
}
