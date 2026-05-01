import type { ConversationRecord } from '@/domain'
import { Icon } from './Icon'

interface Props {
  records: ConversationRecord[]
  status: 'idle' | 'running' | 'succeeded' | 'failed'
  error: string | null
  onClose: () => void
}

export function ConversationHistoryPanel({
  records,
  status,
  error,
  onClose,
}: Props) {
  return (
    <section
      className="rounded-lg border border-primary/20 bg-surface-container/55 p-4 backdrop-blur-md"
      aria-label="Conversation history"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon name="history" className="h-4 w-4 text-primary" />
          <h2 className="font-mono text-[11px] uppercase tracking-[0.2em] text-primary">
            chat.history
          </h2>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-on-surface-variant transition-colors hover:text-primary"
          aria-label="Close history"
          title="Close history"
        >
          <Icon name="close" className="h-4 w-4" />
        </button>
      </div>

      {status === 'running' && (
        <p className="font-mono text-xs text-on-surface-variant">
          Loading redacted history...
        </p>
      )}

      {status === 'failed' && error && (
        <p className="break-words font-mono text-xs text-red-300">{error}</p>
      )}

      {status !== 'running' && records.length === 0 && (
        <p className="font-mono text-xs text-on-surface-variant">
          No persisted conversations yet.
        </p>
      )}

      {records.length > 0 && (
        <div className="flex flex-col gap-2">
          {records.map((record) => (
            <article
              key={record.summary.session_id}
              className="rounded-md border border-white/10 bg-surface-container-high/45 px-3 py-2"
            >
              <div className="flex min-w-0 items-start justify-between gap-3">
                <p className="min-w-0 flex-1 break-words font-mono text-sm text-on-surface">
                  {record.summary.title}
                </p>
                <span className="shrink-0 font-mono text-[10px] uppercase tracking-[0.16em] text-on-surface-muted">
                  {record.summary.message_count} msgs
                </span>
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[10px] uppercase tracking-[0.14em] text-on-surface-muted">
                <span>
                  {record.summary.selected_model.provider}/
                  {record.summary.selected_model.name}
                </span>
                <span>{formatHistoryDate(record.summary.updated_at_unix_ms)}</span>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  )
}

function formatHistoryDate(unixMs: number): string {
  if (!Number.isFinite(unixMs) || unixMs <= 0) return 'unknown date'
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(unixMs))
}
