import { UI_SLASH_COMMAND_SUGGESTIONS } from '@/presentation/commands/slashCommands'
import { Icon } from './Icon'

interface Props {
  onClose: () => void
}

export function SlashCommandHelpPanel({ onClose }: Props) {
  return (
    <section
      className="rounded-lg border border-primary/20 bg-surface-container/55 p-4 backdrop-blur-md"
      aria-label="Slash command help"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon name="terminal" className="h-4 w-4 text-primary" />
          <h2 className="font-mono text-[11px] uppercase tracking-[0.2em] text-primary">
            repl.help
          </h2>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-on-surface-variant transition-colors hover:text-primary"
          aria-label="Close help"
          title="Close help"
        >
          <Icon name="close" className="h-4 w-4" />
        </button>
      </div>

      <div className="grid gap-2 md:grid-cols-2">
        {UI_SLASH_COMMAND_SUGGESTIONS.map((suggestion) => (
          <div
            key={suggestion.command}
            className="min-w-0 rounded border border-white/10 bg-surface-container-high/35 px-3 py-2"
          >
            <div className="flex min-w-0 items-center justify-between gap-3">
              <span className="font-mono text-sm text-on-surface">
                {suggestion.command}
              </span>
              {suggestion.requiresArgument && (
                <span className="shrink-0 rounded border border-primary/20 px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.14em] text-primary/80">
                  arg
                </span>
              )}
            </div>
            <p className="mt-1 break-words text-xs leading-5 text-on-surface-variant">
              {suggestion.title}
            </p>
            {suggestion.aliases && suggestion.aliases.length > 0 && (
              <p className="mt-1 break-words font-mono text-[10px] uppercase tracking-[0.12em] text-on-surface-muted">
                aliases={suggestion.aliases.join(', ')}
              </p>
            )}
          </div>
        ))}
      </div>
    </section>
  )
}
