// Workspace panel: shows context items (files, screen captures, etc.)

import type { ContextItem, ReplToolCatalogItem, ToolRiskLevel } from '@/domain'
import { Icon } from './Icon'

interface Props {
  items: ContextItem[]
  tools: ReplToolCatalogItem[]
}

export function WorkspacePanel({ items, tools }: Props) {
  return (
    <div className="h-full overflow-y-auto p-5 sm:p-8">
      <div className="mx-auto flex max-w-6xl flex-col gap-6">
        <header className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div>
            <p className="mb-2 font-display text-[11px] uppercase tracking-[0.24em] text-primary/80">
              Context Workspace
            </p>
            <h1 className="font-display text-3xl font-semibold tracking-tight text-on-surface">
              Active workspace
            </h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">
              Arquivos, capturas e documentos que alimentam a sessão local do
              agente.
            </p>
          </div>
          <button
            type="button"
            className="desktop-glass-panel inline-flex items-center gap-2 rounded-lg px-4 py-2 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-colors hover:bg-primary/10"
          >
            <Icon name="cloud" className="h-4 w-4" />
            Local environment
          </button>
        </header>

        <section className="desktop-dropzone relative flex min-h-[340px] flex-col items-center justify-center overflow-hidden rounded-xl border-2 border-dashed border-outline-variant/70 p-6 text-center transition-colors hover:border-primary/50">
          <div className="pointer-events-none absolute -right-24 -top-24 h-64 w-64 rounded-full bg-secondary-container/20 blur-[80px]" />
          <div className="mb-6 flex h-20 w-20 items-center justify-center rounded-full border border-white/10 bg-surface-container-highest text-primary transition-transform duration-500 group-hover:scale-110">
            <Icon name="file" className="h-9 w-9 drop-shadow-[0_0_12px_rgba(0,219,233,0.6)]" />
          </div>
          <h2 className="font-display text-2xl font-medium text-on-surface">
            Drop context files here
          </h2>
          <p className="mt-3 max-w-md text-sm leading-6 text-on-surface-variant">
            Injete documentos, trechos de código ou screenshots no contexto
            ativo do Coddy. O backend ainda vai receber o upload real.
          </p>
          <button
            type="button"
            className="mt-6 rounded border border-primary/60 px-5 py-2 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-all hover:bg-primary/10"
          >
            Browse files
          </button>
        </section>

        <section className="flex flex-wrap gap-3">
          {items.length === 0 ? (
            <ContextPill label="No context items yet" muted />
          ) : (
            items.map((item) => (
              <ContextPill
                key={item.id}
                label={item.label}
                sensitive={item.sensitive}
              />
            ))
          )}
        </section>

        <section className="flex flex-col gap-4 border-t border-white/10 pt-5">
          <div className="mb-4 flex items-center justify-between gap-3">
            <div>
              <p className="font-display text-[10px] uppercase tracking-[0.22em] text-primary/70">
                Tool Registry
              </p>
              <h2 className="mt-1 font-display text-lg font-medium text-on-surface">
                Local tools
              </h2>
            </div>
            <span className="rounded border border-primary/20 bg-primary/5 px-2.5 py-1 font-mono text-[11px] text-primary">
              {tools.length}
            </span>
          </div>

          {tools.length === 0 ? (
            <ContextPill label="No tools loaded yet" muted />
          ) : (
            <div className="grid gap-3 xl:grid-cols-2">
              {tools.map((tool) => (
                <ToolRow key={tool.name} tool={tool} />
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  )
}

function ToolRow({ tool }: { tool: ReplToolCatalogItem }) {
  const permissions = tool.permissions.length
    ? tool.permissions.join(', ')
    : 'No explicit permission'
  const requiredFields = requiredInputFields(tool.input_schema)

  return (
    <article className="rounded-lg border border-outline/15 bg-surface-container-highest/40 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate font-mono text-sm text-on-surface">
            {tool.name}
          </h3>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-on-surface-variant">
            {tool.description}
          </p>
        </div>
        <RiskBadge risk={tool.risk_level} />
      </div>

      <div className="mt-3 flex flex-wrap gap-2">
        <MetaPill label={tool.category} />
        <MetaPill label={tool.approval_policy} />
        <MetaPill label={`${tool.timeout_ms}ms`} />
        <MetaPill
          label={
            requiredFields.length
              ? `input: ${requiredFields.join(', ')}`
              : 'input: optional'
          }
        />
      </div>

      <p className="mt-3 truncate font-mono text-[11px] text-on-surface-variant/80">
        {permissions}
      </p>
    </article>
  )
}

function requiredInputFields(schema: ReplToolCatalogItem['input_schema']) {
  const required = schema.required
  if (!Array.isArray(required)) return []
  return required.filter((field): field is string => typeof field === 'string')
}

function RiskBadge({ risk }: { risk: ToolRiskLevel }) {
  const tone = riskTone(risk)

  return (
    <span
      className={`shrink-0 rounded border px-2 py-1 font-mono text-[11px] ${tone}`}
    >
      {risk}
    </span>
  )
}

function MetaPill({ label }: { label: string }) {
  return (
    <span className="rounded border border-white/10 bg-surface-container/60 px-2 py-1 font-mono text-[11px] text-on-surface-variant">
      {label}
    </span>
  )
}

function riskTone(risk: ToolRiskLevel): string {
  switch (risk) {
    case 'Low':
      return 'border-emerald-400/20 bg-emerald-400/10 text-emerald-200'
    case 'Medium':
      return 'border-yellow-400/20 bg-yellow-400/10 text-yellow-200'
    case 'High':
      return 'border-orange-400/20 bg-orange-400/10 text-orange-200'
    case 'Critical':
      return 'border-red-400/20 bg-red-400/10 text-red-200'
  }
}

function ContextPill({
  label,
  sensitive = false,
  muted = false,
}: {
  label: string
  sensitive?: boolean
  muted?: boolean
}) {
  return (
    <div
      className={`flex items-center gap-2 rounded-full border px-3 py-2 font-mono text-sm ${
        sensitive
          ? 'border-yellow-500/20 bg-yellow-500/5 text-yellow-200'
          : muted
            ? 'border-outline/10 bg-surface-container-highest/40 text-on-surface-variant/50'
            : 'border-outline/20 bg-surface-container-highest text-on-surface'
      }`}
    >
      <Icon
        name={sensitive ? 'lock' : 'file'}
        className={`h-4 w-4 ${sensitive ? 'text-yellow-300' : 'text-primary'}`}
      />
      <span className="max-w-[220px] truncate">{label}</span>
    </div>
  )
}
