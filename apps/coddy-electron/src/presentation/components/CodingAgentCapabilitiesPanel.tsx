import type { ReplSession } from '@/domain'
import { Icon, type IconName } from './Icon'

interface Props {
  session: ReplSession
  workspacePath?: string | null
  toolCount?: number
  onClose: () => void
}

type ReadinessTone = 'ready' | 'watch' | 'gap'

type CapabilityRow = {
  label: string
  detail: string
  tone: ReadinessTone
}

type CapabilityGroup = {
  title: string
  icon: IconName
  rows: CapabilityRow[]
}

export function CodingAgentCapabilitiesPanel({
  session,
  workspacePath = null,
  toolCount = 0,
  onClose,
}: Props) {
  const selectedWorkspace = workspacePath?.trim() || null
  const activeRun = session.active_run ?? 'none'
  const model = `${session.selected_model.provider}/${session.selected_model.name}`
  const subagentCount = session.subagent_activity.length

  const groups: CapabilityGroup[] = [
    {
      title: 'agent.loop',
      icon: 'sensors',
      rows: [
        {
          label: 'Plan, act, observe',
          detail:
            'Runs expose phases, tool observations and follow-up responses in session state.',
          tone: session.agent_run ? 'ready' : 'watch',
        },
        {
          label: 'Coding workflows',
          detail:
            '/code, /plan, /review and /test inject guarded workflow prompts for coding tasks.',
          tone: 'ready',
        },
        {
          label: 'Stop control',
          detail:
            'Esc is routed to active runs, streaming, voice capture and speech cancellation.',
          tone: session.status === 'Idle' ? 'ready' : 'watch',
        },
      ],
    },
    {
      title: 'tools.security',
      icon: 'lock',
      rows: [
        {
          label: 'Tool registry',
          detail: `${toolCount} registered tools are visible to the current session.`,
          tone: toolCount > 0 ? 'ready' : 'watch',
        },
        {
          label: 'Provider-safe names',
          detail:
            'Dotted tools are decoded through provider-safe aliases before execution.',
          tone: 'ready',
        },
        {
          label: 'Secrets and credentials',
          detail:
            'Provider keys remain request-scoped or stored through secure UI flows with redacted diagnostics.',
          tone: 'ready',
        },
      ],
    },
    {
      title: 'workspace.context',
      icon: 'file',
      rows: [
        {
          label: 'Workspace root',
          detail: selectedWorkspace ?? 'Select a workspace before codebase tasks.',
          tone: selectedWorkspace ? 'ready' : 'gap',
        },
        {
          label: 'Session continuity',
          detail:
            '/history resumes persisted conversations and /new starts a clean session.',
          tone: 'ready',
        },
        {
          label: 'Current run',
          detail: activeRun,
          tone: activeRun === 'none' ? 'watch' : 'ready',
        },
      ],
    },
    {
      title: 'quality.evals',
      icon: 'cpu',
      rows: [
        {
          label: 'Prompt battery',
          detail:
            'Quality gates can exercise analysis, tools, agents and provider reliability.',
          tone: 'ready',
        },
        {
          label: 'Subagent contracts',
          detail: `${subagentCount} subagent activities are currently represented in session state.`,
          tone: subagentCount > 0 ? 'ready' : 'watch',
        },
        {
          label: 'Top-tier gap',
          detail:
            'Next hardening target: richer sandbox expansion, hooks and MCP permission bridge.',
          tone: 'gap',
        },
      ],
    },
  ]

  return (
    <section
      className="rounded-lg border border-primary/20 bg-surface-container/55 p-4 backdrop-blur-md"
      aria-label="Coding agent capabilities"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon name="bot" className="h-4 w-4 text-primary" />
          <div className="min-w-0">
            <h2 className="font-mono text-[11px] uppercase tracking-[0.2em] text-primary">
              agent.capabilities
            </h2>
            <p className="mt-1 truncate font-mono text-[10px] uppercase tracking-[0.14em] text-on-surface-muted">
              model={model}
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-on-surface-variant transition-colors hover:text-primary"
          aria-label="Close capabilities"
          title="Close capabilities"
        >
          <Icon name="close" className="h-4 w-4" />
        </button>
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        {groups.map((group) => (
          <div
            key={group.title}
            className="min-w-0 rounded border border-white/10 bg-surface-container-high/35 px-3 py-3"
          >
            <div className="mb-2 flex items-center gap-2">
              <Icon name={group.icon} className="h-4 w-4 text-primary/80" />
              <h3 className="font-mono text-[10px] uppercase tracking-[0.18em] text-primary/80">
                {group.title}
              </h3>
            </div>
            <div className="flex flex-col gap-2">
              {group.rows.map((row) => (
                <div key={row.label} className="min-w-0">
                  <div className="mb-1 flex min-w-0 items-center justify-between gap-3">
                    <span className="min-w-0 truncate text-sm font-medium text-on-surface">
                      {row.label}
                    </span>
                    <ReadinessBadge tone={row.tone} />
                  </div>
                  <p className="break-words text-xs leading-5 text-on-surface-variant">
                    {row.detail}
                  </p>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}

function ReadinessBadge({ tone }: { tone: ReadinessTone }) {
  const className =
    tone === 'ready'
      ? 'border-emerald-300/30 bg-emerald-300/10 text-emerald-200'
      : tone === 'watch'
        ? 'border-primary/25 bg-primary/10 text-primary'
        : 'border-amber-300/30 bg-amber-300/10 text-amber-200'

  return (
    <span
      className={`shrink-0 rounded border px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.14em] ${className}`}
    >
      {tone}
    </span>
  )
}
