import { useCallback, useState } from 'react'
import {
  formatAgentRunRecoveryDiagnostics,
  type AgentRunRecoveryNotice,
} from '@/domain'
import { Icon } from './Icon'

interface Props {
  notice: AgentRunRecoveryNotice
  compact?: boolean
}

export function AgentRunRecoveryCard({
  notice,
  compact = false,
}: Props) {
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>(
    'idle',
  )

  const handleCopyDiagnostics = useCallback(async () => {
    const writeText = navigator.clipboard?.writeText

    if (!writeText) {
      setCopyState('failed')
      return
    }

    try {
      await writeText.call(
        navigator.clipboard,
        formatAgentRunRecoveryDiagnostics(notice),
      )
      setCopyState('copied')
    } catch {
      setCopyState('failed')
    }
  }, [notice])

  const copyLabel = copyState === 'copied'
    ? 'copied'
    : copyState === 'failed'
      ? 'copy failed'
      : 'copy diagnostics'

  return (
    <div
      className={`rounded-md border border-amber-300/25 bg-amber-500/10 px-4 py-3 ${
        compact ? '' : 'mb-3 ml-7'
      }`}
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-xs font-bold text-amber-200">
            {notice.title}
          </span>
          <span className="rounded border border-amber-200/20 px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.12em] text-amber-100/70">
            {notice.technicalCode}
          </span>
        </div>
        <button
          type="button"
          onClick={handleCopyDiagnostics}
          className="flex items-center gap-1.5 rounded border border-amber-200/20 px-2 py-1 font-mono text-[11px] text-amber-100/75 transition-colors hover:border-amber-200/40 hover:text-amber-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-200"
        >
          <Icon name="copy" className="h-3.5 w-3.5" />
          {copyLabel}
        </button>
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
