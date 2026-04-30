import type { PermissionReply, PermissionRequest } from '@/domain'
import { Icon } from '@/presentation/components/Icon'

interface Props {
  request: PermissionRequest
  onReply: (requestId: string, reply: PermissionReply) => void
}

export function ToolApprovalPanel({ request, onReply }: Props) {
  const target = request.patterns.join(', ')

  return (
    <section className="rounded-xl border border-orange-300/25 bg-orange-500/10 px-4 py-3 text-on-surface shadow-[0_0_40px_rgba(251,146,60,0.08)] backdrop-blur-2xl">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0 flex-1">
          <div className="mb-2 flex items-center gap-2 font-display text-[11px] uppercase tracking-[0.2em] text-orange-200">
            <Icon name="lock" className="h-4 w-4" />
            Tool approval
          </div>
          <p className="break-words font-mono text-sm text-on-surface">
            {request.tool_name}
          </p>
          <p className="mt-1 break-words font-mono text-xs text-on-surface-variant/75">
            {request.permission} // {request.risk_level} // {target}
          </p>
        </div>

        <div className="flex flex-wrap gap-2">
          <ApprovalButton
            label="Once"
            tone="primary"
            onClick={() => onReply(request.id, 'Once')}
          />
          <ApprovalButton
            label="Always"
            tone="neutral"
            onClick={() => onReply(request.id, 'Always')}
          />
          <ApprovalButton
            label="Reject"
            tone="danger"
            onClick={() => onReply(request.id, 'Reject')}
          />
        </div>
      </div>
    </section>
  )
}

function ApprovalButton({
  label,
  tone,
  onClick,
}: {
  label: string
  tone: 'primary' | 'neutral' | 'danger'
  onClick: () => void
}) {
  const toneClass =
    tone === 'primary'
      ? 'border-primary/45 bg-primary/10 text-primary hover:bg-primary/15'
      : tone === 'danger'
        ? 'border-red-300/35 bg-red-500/10 text-red-200 hover:bg-red-500/15'
        : 'border-white/15 bg-surface-container-high/70 text-on-surface-variant hover:text-on-surface'

  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-full border px-3 py-1 font-mono text-xs transition-colors ${toneClass}`}
    >
      {label}
    </button>
  )
}
