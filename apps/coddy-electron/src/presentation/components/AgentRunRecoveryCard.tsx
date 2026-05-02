import type { AgentRunRecoveryNotice } from '@/domain'

interface Props {
  notice: AgentRunRecoveryNotice
  compact?: boolean
}

export function AgentRunRecoveryCard({
  notice,
  compact = false,
}: Props) {
  return (
    <div
      className={`rounded-md border border-amber-300/25 bg-amber-500/10 px-4 py-3 ${
        compact ? '' : 'mb-3 ml-7'
      }`}
    >
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <span className="font-mono text-xs font-bold text-amber-200">
          {notice.title}
        </span>
        <span className="rounded border border-amber-200/20 px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.12em] text-amber-100/70">
          {notice.technicalCode}
        </span>
      </div>
      <p className="break-words font-mono text-xs text-amber-100/85">
        {notice.message}
      </p>
      <p className="mt-2 break-words font-mono text-xs text-on-surface-variant">
        {notice.action}
      </p>
    </div>
  )
}
