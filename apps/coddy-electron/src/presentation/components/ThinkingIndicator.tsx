import { Icon } from '@/presentation/components/Icon'

export type ThinkingAnimation = 'pulse' | 'scan' | 'orbit'

interface Props {
  animation?: ThinkingAnimation
  label?: string
  showCancelHint?: boolean
}

export function ThinkingIndicator({
  animation = 'scan',
  label = 'coddy_thinking',
  showCancelHint = true,
}: Props) {
  const motionClass =
    animation === 'orbit'
      ? 'animate-spin'
      : animation === 'pulse'
        ? 'animate-pulse'
        : 'thinking-scan'

  return (
    <div className="flex items-start gap-4">
      <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary/70 bg-primary/10 text-primary">
        <Icon name="bot" className={`h-4 w-4 ${motionClass}`} />
      </div>
      <div className="desktop-glass-panel max-w-3xl flex-1 overflow-hidden rounded-lg px-5 py-4">
        <div className="mb-3 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
          {label}
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-primary/20">
          <span
            className={`block h-full rounded-full bg-primary/70 ${
              animation === 'pulse'
                ? 'w-full animate-pulse'
                : 'w-1/3 thinking-loading-bar'
            }`}
          />
        </div>
        {showCancelHint && (
          <p className="mt-3 font-mono text-[11px] text-on-surface-variant/70">
            Pressione (Esc) para parar.
          </p>
        )}
      </div>
    </div>
  )
}
