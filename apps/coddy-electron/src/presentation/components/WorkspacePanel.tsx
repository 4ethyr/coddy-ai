// Workspace panel: shows context items, harness metrics and local tools.

import { useState } from 'react'
import type {
  ContextItem,
  MultiagentEvalRequest,
  MultiagentEvalResult,
  ReplToolCatalogItem,
  ToolRiskLevel,
} from '@/domain'
import type { MultiagentEvalStatus } from '@/presentation/hooks/useSession'
import { Icon } from './Icon'

interface Props {
  items: ContextItem[]
  tools: ReplToolCatalogItem[]
  multiagentEval?: MultiagentEvalResult | null
  multiagentEvalStatus?: MultiagentEvalStatus
  multiagentEvalError?: string | null
  onRunMultiagentEval?: (request: MultiagentEvalRequest) => void
}

export function WorkspacePanel({
  items,
  tools,
  multiagentEval,
  multiagentEvalStatus = 'idle',
  multiagentEvalError = null,
  onRunMultiagentEval,
}: Props) {
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

        {(onRunMultiagentEval || multiagentEval || multiagentEvalError) && (
          <MultiagentEvalPanel
            result={multiagentEval}
            status={multiagentEvalStatus}
            error={multiagentEvalError}
            onRun={onRunMultiagentEval}
          />
        )}

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

function MultiagentEvalPanel({
  result,
  status,
  error,
  onRun,
}: {
  result?: MultiagentEvalResult | null
  status: MultiagentEvalStatus
  error?: string | null
  onRun?: (request: MultiagentEvalRequest) => void
}) {
  const [baselinePath, setBaselinePath] = useState('')
  const [writeBaselinePath, setWriteBaselinePath] = useState('')
  const suite = result?.suite
  const comparison = result?.comparison
  const score = suite ? Math.round(suite.score) : '--'
  const passed = suite?.passed ?? '--'
  const failed = suite?.failed ?? '--'
  const delta =
    comparison && Number.isFinite(comparison.scoreDelta)
      ? formatSignedNumber(comparison.scoreDelta)
      : '--'
  const disabled = status === 'running' || !onRun

  const handleRun = () => {
    onRun?.(buildEvalRequest(baselinePath, writeBaselinePath))
  }

  return (
    <section className="flex flex-col gap-4 border-t border-white/10 pt-5">
      <div className="flex flex-col justify-between gap-3 sm:flex-row sm:items-center">
        <div>
          <p className="font-display text-[10px] uppercase tracking-[0.22em] text-primary/70">
            Agent Harness
          </p>
          <h2 className="mt-1 font-display text-lg font-medium text-on-surface">
            Multiagent eval
          </h2>
        </div>
        <button
          type="button"
          onClick={handleRun}
          disabled={disabled}
          aria-label="Run multiagent eval"
          className="desktop-glass-panel inline-flex h-9 items-center justify-center gap-2 rounded-lg px-4 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-colors hover:bg-primary/10 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Icon
            name={status === 'running' ? 'cpu' : 'bot'}
            className={`h-4 w-4 ${status === 'running' ? 'animate-pulse' : ''}`}
          />
          {status === 'running' ? 'Running' : 'Run eval'}
        </button>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <EvalMetric label="score" value={String(score)} tone="primary" />
        <EvalMetric label="passed" value={String(passed)} tone="success" />
        <EvalMetric label="failed" value={String(failed)} tone="danger" />
        <EvalMetric label="delta" value={delta} tone={deltaTone(comparison)} />
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        <EvalPathInput
          label="baseline"
          value={baselinePath}
          onChange={setBaselinePath}
        />
        <EvalPathInput
          label="write baseline"
          value={writeBaselinePath}
          onChange={setWriteBaselinePath}
        />
      </div>

      {comparison && (
        <div className="rounded-lg border border-outline/15 bg-surface-container-highest/35 px-3 py-2 font-mono text-xs text-on-surface-variant">
          <div className="flex flex-wrap items-center gap-2">
            <span className={comparisonTone(comparison.status)}>
              baseline {comparison.status}
            </span>
            <span>
              {Math.round(comparison.previousScore)}
              {' -> '}
              {Math.round(comparison.currentScore)}
            </span>
          </div>
          {comparison.regressions.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-2">
              {comparison.regressions.map((regression) => (
                <MetaPill key={regression} label={regression} />
              ))}
            </div>
          )}
        </div>
      )}

      {error && (
        <p
          role="alert"
          className="rounded border border-red-400/20 bg-red-400/10 px-3 py-2 font-mono text-xs text-red-200"
        >
          {error}
        </p>
      )}
    </section>
  )
}

function EvalPathInput({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <label className="flex min-w-0 flex-col gap-2 rounded-lg border border-outline/15 bg-surface-container-highest/30 px-3 py-2">
      <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-on-surface-variant/70">
        {label}
      </span>
      <input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-8 min-w-0 bg-transparent font-mono text-xs text-on-surface outline-none placeholder:text-on-surface-variant/35"
        placeholder="/tmp/coddy-multiagent-baseline.json"
      />
    </label>
  )
}

function EvalMetric({
  label,
  value,
  tone,
}: {
  label: string
  value: string
  tone: 'primary' | 'success' | 'danger' | 'muted'
}) {
  return (
    <div className="rounded-lg border border-outline/15 bg-surface-container-highest/40 px-3 py-3">
      <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-on-surface-variant/70">
        {label}
      </p>
      <p className={`mt-2 font-display text-2xl font-semibold ${metricTone(tone)}`}>
        {value}
      </p>
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

function buildEvalRequest(
  baselinePath: string,
  writeBaselinePath: string,
): MultiagentEvalRequest {
  const request: MultiagentEvalRequest = {}
  const baseline = baselinePath.trim()
  const writeBaseline = writeBaselinePath.trim()
  if (baseline) request.baseline = baseline
  if (writeBaseline) request.writeBaseline = writeBaseline
  return request
}

function formatSignedNumber(value: number): string {
  return value > 0 ? `+${value}` : String(value)
}

function deltaTone(
  comparison?: MultiagentEvalResult['comparison'],
): 'primary' | 'success' | 'danger' | 'muted' {
  if (!comparison) return 'muted'
  if (comparison.scoreDelta > 0) return 'success'
  if (comparison.scoreDelta < 0) return 'danger'
  return 'primary'
}

function metricTone(tone: 'primary' | 'success' | 'danger' | 'muted'): string {
  switch (tone) {
    case 'primary':
      return 'text-primary'
    case 'success':
      return 'text-emerald-200'
    case 'danger':
      return 'text-red-200'
    case 'muted':
      return 'text-on-surface-variant'
  }
}

function comparisonTone(status: 'passed' | 'failed'): string {
  return status === 'passed' ? 'text-emerald-200' : 'text-red-200'
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
