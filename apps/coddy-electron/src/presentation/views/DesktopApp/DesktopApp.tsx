// presentation/views/DesktopApp/DesktopApp.tsx
// Advanced mode: sidebar + conversation + workspace panels.
// Uses the same useSession hook as FloatingTerminal.

import { useCallback, useState, type ComponentProps } from 'react'
import type {
  FloatingAppearanceSettings,
  ModelThinkingSettings,
} from '@/application'
import { loadSettings, saveSettings } from '@/application'
import { getRuntimeChatCapability } from '@/domain'
import { useSessionContext } from '@/presentation/hooks'
import { Sidebar, type DesktopTab } from '@/presentation/components/Sidebar'
import { ConversationPanel } from '@/presentation/components/ConversationPanel'
import { WorkspacePanel } from '@/presentation/components/WorkspacePanel'
import { ModelSelector } from '@/presentation/components/ModelSelector'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'
import { FloatingSettingsModal } from '@/presentation/components/FloatingSettingsModal'
import { Icon } from '@/presentation/components/Icon'

export function DesktopApp() {
  const {
    session,
    toolCatalog,
    multiagentEval,
    multiagentEvalStatus,
    multiagentEvalError,
    connecting,
    error,
    ask,
    selectModel,
    listProviderModels,
    runMultiagentEval,
    openUi,
    replyPermission,
  } = useSessionContext()
  const [activeTab, setActiveTab] = useState<DesktopTab>('chat')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [appearance, setAppearance] = useState<FloatingAppearanceSettings>(
    () => loadSettings().floatingAppearance,
  )
  const [modelThinking, setModelThinking] = useState<ModelThinkingSettings>(
    () => loadSettings().modelThinking,
  )

  const handleClose = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:close')
    }
  }, [])

  const handleMinimize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:minimize')
    }
  }, [])

  const handleMaximize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:maximize')
    }
  }, [])

  const handleAppearanceChange = useCallback(
    (next: FloatingAppearanceSettings) => {
      setAppearance(next)
      saveSettings({ floatingAppearance: next })
    },
    [],
  )

  const handleModelThinkingChange = useCallback(
    (next: ModelThinkingSettings) => {
      setModelThinking(next)
      saveSettings({ modelThinking: next })
    },
    [],
  )

  return (
    <div className="desktop-shell relative flex h-screen overflow-hidden bg-background text-on-surface">
      <Sidebar
        activeTab={activeTab}
        onTabChange={setActiveTab}
        connected={!connecting}
        status={session.status}
        mode={session.mode}
        onOpenMode={(mode) => {
          void openUi(mode)
        }}
      />

      <main className="relative flex min-w-0 flex-1 flex-col">
        <header className="desktop-topbar flex h-12 flex-shrink-0 items-center justify-between border-b border-white/10 bg-zinc-950/60 px-4 backdrop-blur-lg sm:px-6">
          <div className="flex min-w-0 items-center gap-4">
            <span className="font-display text-lg font-bold uppercase tracking-tight text-primary drop-shadow-[0_0_8px_rgba(0,240,255,0.5)]">
              Coddy Core
            </span>
            <div className="hidden items-center gap-2 rounded border border-white/5 bg-surface-container/50 px-3 py-1 md:flex">
              <span className="h-2 w-2 rounded-full bg-primary shadow-[0_0_8px_rgba(0,219,233,0.8)]" />
              <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-zinc-400">
                {session.selected_model.name}
              </span>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <StatusIndicator status={session.status} />
            <button
              type="button"
              onClick={() => setActiveTab('settings')}
              className="text-zinc-500 transition-colors hover:text-primary"
              title="Config"
              aria-label="Open config"
            >
              <Icon name="settings" className="h-5 w-5" />
            </button>
            <button
              type="button"
              onClick={() => {
                void openUi('FloatingTerminal')
              }}
              className="text-zinc-500 transition-colors hover:text-primary"
              title="Floating terminal"
              aria-label="Switch to floating terminal"
            >
              <Icon name="terminal" className="h-5 w-5" />
            </button>
            <div className="ml-1 flex items-center gap-1">
              <WindowButton
                label="Minimize"
                icon="minimize"
                onClick={handleMinimize}
              />
              <WindowButton
                label="Maximize"
                icon="maximize"
                onClick={handleMaximize}
              />
              <WindowButton
                label="Close"
                icon="close"
                tone="danger"
                onClick={handleClose}
              />
            </div>
          </div>
        </header>

        {error && (
          <div className="border-b border-red-500/20 bg-red-500/10 px-5 py-2 font-mono text-sm text-red-300">
            {error}
          </div>
        )}

        {connecting && (
          <div className="flex items-center gap-2 border-b border-primary/10 px-5 py-2 font-mono text-sm text-on-surface-variant">
            <span className="h-2 w-2 animate-pulse rounded-full bg-primary" />
            Connecting to daemon...
          </div>
        )}

        <div className="relative min-h-0 flex-1 overflow-hidden">
          {activeTab === 'chat' && (
            <ConversationPanel
              session={session}
              onSend={ask}
              onPermissionReply={(requestId, reply) => {
                void replyPermission(requestId, reply)
              }}
              thinkingAnimation={modelThinking.animation}
            />
          )}

          {activeTab === 'workspace' && (
            <WorkspacePanel
              items={session.workspace_context}
              tools={toolCatalog}
              multiagentEval={multiagentEval}
              multiagentEvalStatus={multiagentEvalStatus}
              multiagentEvalError={multiagentEvalError}
              onRunMultiagentEval={(request) => {
                void runMultiagentEval(request)
              }}
            />
          )}

          {activeTab === 'models' && (
            <ModelsTab
              model={session.selected_model}
              onLoadModels={listProviderModels}
              onSelect={(model) => {
                void selectModel(model, 'Chat')
              }}
            />
          )}

          {activeTab === 'settings' && (
            <SettingsTab
              appearance={appearance}
              modelThinking={modelThinking}
              onOpenAppearance={() => setSettingsOpen(true)}
              onModelThinkingChange={handleModelThinkingChange}
            />
          )}
        </div>
      </main>

      {settingsOpen && (
        <FloatingSettingsModal
          value={appearance}
          onChange={handleAppearanceChange}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  )
}

type WindowButtonProps = {
  label: string
  icon: 'close' | 'minimize' | 'maximize'
  tone?: 'default' | 'danger'
  onClick: () => void
}

function WindowButton({
  label,
  icon,
  tone = 'default',
  onClick,
}: WindowButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-5 w-5 items-center justify-center rounded-full transition-colors ${
        tone === 'danger'
          ? 'bg-red-500/10 text-red-300/70 hover:text-red-300'
          : 'bg-surface-container-high/70 text-on-surface-variant/70 hover:text-on-surface'
      }`}
      title={label}
      aria-label={label}
    >
      <Icon name={icon} className="h-3.5 w-3.5" />
    </button>
  )
}

function ModelsTab({
  model,
  onLoadModels,
  onSelect,
}: {
  model: { provider: string; name: string }
  onLoadModels: ComponentProps<typeof ModelSelector>['onLoadModels']
  onSelect: (model: { provider: string; name: string }) => void
}) {
  const runtimeChat = getRuntimeChatCapability(model.provider)
  const runtimeTone =
    runtimeChat.status === 'supported' ? 'text-primary' : 'text-yellow-200'
  const pipelineValue =
    runtimeChat.status === 'supported' ? 'Runtime' : 'Discovery'

  return (
    <div className="h-full overflow-y-auto p-5 sm:p-8">
      <div className="mx-auto flex max-w-6xl flex-col gap-6">
        <section className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div>
            <p className="mb-2 font-display text-[11px] uppercase tracking-[0.24em] text-primary/80">
              Daemon Pipeline
            </p>
            <h1 className="font-display text-3xl font-semibold tracking-tight text-on-surface">
              Model routing
            </h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">
              Controle o modelo usado pelo chat do Coddy. O daemon sincroniza
              sessão, mensagens, tools e configuração por eventos para manter
              terminal flutuante e desktop alinhados.
            </p>
          </div>
          <ModelSelector
            model={model}
            onLoadModels={onLoadModels}
            onSelect={onSelect}
          />
        </section>

        <div className="grid gap-4 md:grid-cols-3">
          <MetricCard label="CPU CORE" value="Nominal" icon="cpu" tone="primary" />
          <MetricCard label="PIPELINE" value={pipelineValue} icon="sensors" tone="secondary" />
          <MetricCard label="BACKEND" value={model.provider} icon="cloud" tone="neutral" />
        </div>

        <section className="desktop-glass-panel overflow-hidden rounded-xl">
          <div className="flex items-center justify-between border-b border-white/10 px-5 py-4">
            <div>
              <h2 className="font-display text-lg font-medium text-on-surface">
                Active daemon
              </h2>
              <p className="mt-1 font-mono text-xs text-on-surface-variant/70">
                {runtimeChat.description}
              </p>
            </div>
            <span
              className={`rounded border border-secondary/30 px-2 py-1 font-mono text-[10px] uppercase tracking-[0.18em] ${runtimeTone}`}
            >
              {runtimeChat.label}
            </span>
          </div>
          <div className="flex flex-col gap-3 p-4">
            <div className="flex items-center justify-between rounded-lg border border-white/5 bg-surface-container-low/70 p-4">
              <div className="flex items-center gap-4">
                <span className="h-2 w-2 rounded-full bg-primary shadow-[0_0_8px_rgba(0,219,233,0.8)]" />
                <div>
                  <p className="font-display text-base text-on-surface">
                    {model.name}
                  </p>
                  <p className="mt-1 font-mono text-xs text-on-surface-variant/60">
                    provider={model.provider}; keep_alive=15m
                  </p>
                </div>
              </div>
              <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-primary">
                active
              </span>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}

function SettingsTab({
  appearance,
  modelThinking,
  onOpenAppearance,
  onModelThinkingChange,
}: {
  appearance: FloatingAppearanceSettings
  modelThinking: ModelThinkingSettings
  onOpenAppearance: () => void
  onModelThinkingChange: (next: ModelThinkingSettings) => void
}) {
  const setThinking = (patch: Partial<ModelThinkingSettings>) => {
    onModelThinkingChange({ ...modelThinking, ...patch })
  }

  return (
    <div className="h-full overflow-y-auto p-5 sm:p-8">
      <div className="mx-auto max-w-5xl">
        <p className="mb-2 font-display text-[11px] uppercase tracking-[0.24em] text-primary/80">
          System Configuration
        </p>
        <h1 className="font-display text-3xl font-semibold tracking-tight text-on-surface">
          Interface controls
        </h1>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">
          Ajuste aparência, transparência e glassmorphism do terminal flutuante
          sem reiniciar a sessão.
        </p>

        <section className="desktop-glass-panel mt-6 rounded-xl p-5">
          <div className="flex flex-col justify-between gap-5 md:flex-row md:items-center">
            <div>
              <h2 className="font-display text-lg text-on-surface">
                Floating terminal appearance
              </h2>
              <div className="mt-4 grid gap-3 font-mono text-xs text-on-surface-variant sm:grid-cols-2">
                <span>blur={appearance.blurPx}px</span>
                <span>opacity={Math.round(appearance.transparency * 100)}%</span>
                <span>glass={Math.round(appearance.glassIntensity * 100)}%</span>
                <span>accent={appearance.accentColor}</span>
              </div>
            </div>
            <button
              type="button"
              onClick={onOpenAppearance}
              className="rounded border border-primary/40 px-4 py-2 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-colors hover:bg-primary/10"
            >
              Open controls
            </button>
          </div>
        </section>

        <section className="desktop-glass-panel mt-4 rounded-xl p-5">
          <div className="flex flex-col gap-5">
            <div className="flex flex-col justify-between gap-3 md:flex-row md:items-center">
              <div>
                <h2 className="font-display text-lg text-on-surface">
                  Model thinking
                </h2>
                <p className="mt-1 max-w-2xl text-sm leading-6 text-on-surface-variant">
                  Configure o comportamento visual e o orçamento pretendido
                  para respostas com raciocínio mais profundo.
                </p>
              </div>
              <button
                type="button"
                aria-pressed={modelThinking.enabled}
                onClick={() => setThinking({ enabled: !modelThinking.enabled })}
                className={`rounded-full border px-4 py-2 font-mono text-xs transition-colors ${
                  modelThinking.enabled
                    ? 'border-primary/45 bg-primary/10 text-primary'
                    : 'border-white/15 bg-surface-container-high/70 text-on-surface-variant'
                }`}
              >
                {modelThinking.enabled ? 'Enabled' : 'Disabled'}
              </button>
            </div>

            <div className="grid gap-4 md:grid-cols-3">
              <SegmentedControl
                label="Effort"
                value={modelThinking.effort}
                options={['minimal', 'balanced', 'deep']}
                onChange={(effort) => setThinking({ effort })}
              />
              <SegmentedControl
                label="Animation"
                value={modelThinking.animation}
                options={['pulse', 'scan', 'orbit']}
                onChange={(animation) => setThinking({ animation })}
              />
              <label className="flex flex-col gap-2 rounded-lg border border-white/10 bg-surface-container-low/50 p-4">
                <span className="font-display text-[10px] uppercase tracking-[0.2em] text-on-surface-variant">
                  Budget
                </span>
                <input
                  type="range"
                  min="0"
                  max="32768"
                  step="512"
                  value={modelThinking.budgetTokens}
                  onChange={(event) =>
                    setThinking({
                      budgetTokens: Number(event.currentTarget.value),
                    })
                  }
                  className="accent-primary"
                />
                <span className="font-mono text-xs text-primary">
                  {modelThinking.budgetTokens} tokens
                </span>
              </label>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}

function SegmentedControl<TValue extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: TValue
  options: readonly TValue[]
  onChange: (value: TValue) => void
}) {
  return (
    <div className="rounded-lg border border-white/10 bg-surface-container-low/50 p-4">
      <p className="mb-3 font-display text-[10px] uppercase tracking-[0.2em] text-on-surface-variant">
        {label}
      </p>
      <div className="flex flex-wrap gap-2">
        {options.map((option) => (
          <button
            key={option}
            type="button"
            onClick={() => onChange(option)}
            className={`rounded-full border px-3 py-1 font-mono text-xs capitalize transition-colors ${
              option === value
                ? 'border-primary/45 bg-primary/10 text-primary'
                : 'border-white/10 bg-surface-container-high/50 text-on-surface-variant hover:text-on-surface'
            }`}
          >
            {option}
          </button>
        ))}
      </div>
    </div>
  )
}

function MetricCard({
  label,
  value,
  icon,
  tone,
}: {
  label: string
  value: string
  icon: 'cloud' | 'cpu' | 'sensors'
  tone: 'primary' | 'secondary' | 'neutral'
}) {
  const color =
    tone === 'primary'
      ? 'text-primary'
      : tone === 'secondary'
        ? 'text-secondary'
        : 'text-on-surface'

  return (
    <div className="desktop-glass-panel flex items-center gap-4 rounded-xl p-5">
      <span className={`flex h-12 w-12 items-center justify-center rounded-full border border-white/10 bg-surface-container-high/70 ${color}`}>
        <Icon name={icon} className="h-5 w-5" />
      </span>
      <div>
        <p className="font-display text-[10px] uppercase tracking-[0.2em] text-on-surface-variant">
          {label}
        </p>
        <p className="mt-1 font-mono text-sm text-on-surface">{value}</p>
      </div>
    </div>
  )
}
