// presentation/views/FloatingTerminal/FloatingTerminal.tsx
// Main REPL view: floating terminal with glass aesthetic.
// Matches the visual reference in repl_ui/floating_terminal_coding_interaction/

import { useRef, useEffect, useCallback, useState } from 'react'
import type { CSSProperties } from 'react'
import type { ScreenAssistMode } from '@/domain'
import type { FloatingAppearanceSettings } from '@/application'
import { loadSettings, saveSettings } from '@/application'
import { useSessionContext } from '@/presentation/hooks'
import {
  MarkdownContent,
  MessageBubble,
} from '@/presentation/components/MessageBubble'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'
import { InputBar } from '@/presentation/components/InputBar'
import { ModelSelector } from '@/presentation/components/ModelSelector'
import { VoiceButton } from '@/presentation/components/VoiceButton'
import { ThinkingIndicator } from '@/presentation/components/ThinkingIndicator'
import { ToolApprovalPanel } from '@/presentation/components/ToolApprovalPanel'
import { AssessmentConfirmModal } from '@/presentation/components/AssessmentConfirmModal'
import { FloatingSettingsModal } from '@/presentation/components/FloatingSettingsModal'
import { SelectionCopyRegion } from '@/presentation/components/SelectionCopyRegion'
import { ConversationHistoryPanel } from '@/presentation/components/ConversationHistoryPanel'
import { SessionStatusPanel } from '@/presentation/components/SessionStatusPanel'
import { Icon } from '@/presentation/components/Icon'
import {
  persistDesktopTab,
  resolveUiSlashCommand,
} from '@/presentation/commands/slashCommands'

export function FloatingTerminal() {
  const {
    session,
    toolCatalog,
    activeWorkspacePath,
    connecting,
    reconnecting,
    error,
    ask,
    newSession,
    openConversation,
    reconnect,
    selectModel,
    listProviderModels,
    openUi,
    captureVoice,
    cancelVoiceCapture,
    captureAndExplain,
    dismissConfirmation,
    replyPermission,
    conversationHistory,
    conversationHistoryStatus,
    conversationHistoryError,
    loadConversationHistory,
  } =
    useSessionContext()
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const [pendingScreenAssistMode, setPendingScreenAssistMode] =
    useState<ScreenAssistMode | null>(null)
  const [confirmationDismissed, setConfirmationDismissed] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [expanded, setExpanded] = useState(false)
  const [historyOpen, setHistoryOpen] = useState(false)
  const [statusOpen, setStatusOpen] = useState(false)
  const [appearance, setAppearance] = useState<FloatingAppearanceSettings>(
    () => loadSettings().floatingAppearance,
  )
  const [speakVoiceResponses, setSpeakVoiceResponses] = useState(
    () => loadSettings().speakVoiceResponses,
  )
  const [thinkingAnimation] = useState(
    () => loadSettings().modelThinking.animation,
  )
  const toolActivity = session.tool_activity ?? []
  const subagentActivity = session.subagent_activity ?? []

  // Auto-scroll to bottom on new messages or streaming tokens
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView?.({ behavior: 'smooth' })
  }, [session.messages.length, session.streaming_text])

  useEffect(() => {
    if (session.status !== 'AwaitingConfirmation') {
      setConfirmationDismissed(false)
    }
  }, [session.status])

  const handleClose = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:close')
    }
  }, [])

  const handleMaximize = useCallback(async () => {
    if (typeof window !== 'undefined' && window.replApi) {
      const result = await window.replApi.invoke('window:maximize')
      if (result && typeof result === 'object' && 'maximized' in result) {
        setExpanded(Boolean(result.maximized))
      }
    }
  }, [])

  const handleMinimize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:minimize')
    }
  }, [])

  const handleAppearanceChange = useCallback(
    (next: FloatingAppearanceSettings) => {
      setAppearance(next)
      saveSettings({ floatingAppearance: next })
    },
    [],
  )

  const handleSend = useCallback(
    (text: string) => {
      const command = resolveUiSlashCommand(text)
      if (!command) {
        void ask(text)
        return
      }

      if (command.kind === 'open-settings') {
        setSettingsOpen(true)
        return
      }

      if (command.kind === 'agent-workflow') {
        void ask(command.prompt)
        return
      }

      if (command.kind === 'new-session') {
        setHistoryOpen(false)
        setStatusOpen(false)
        void newSession()
        return
      }

      if (command.kind === 'set-speak') {
        setSpeakVoiceResponses(command.enabled)
        saveSettings({ speakVoiceResponses: command.enabled })
        return
      }

      if (command.kind === 'open-history') {
        setStatusOpen(false)
        setHistoryOpen(true)
        void loadConversationHistory()
        return
      }

      if (command.kind === 'show-status') {
        setHistoryOpen(false)
        setStatusOpen(true)
        return
      }

      persistDesktopTab(command.tab)
      void openUi('DesktopApp')
    },
    [ask, loadConversationHistory, newSession, openUi],
  )

  const handleOpenConversation = useCallback(
    (sessionId: string) => {
      setHistoryOpen(false)
      void openConversation(sessionId)
    },
    [openConversation],
  )

  const handleCaptureVoice = useCallback(
    () => captureVoice({ speakResponse: speakVoiceResponses }),
    [captureVoice, speakVoiceResponses],
  )

  const terminalStyle = {
    '--coddy-terminal-opacity': String(appearance.transparency),
    '--coddy-terminal-blur': `${appearance.blurPx}px`,
    '--coddy-terminal-glass-primary': hexToRgba(
      appearance.glassPrimaryColor,
      appearance.glassIntensity,
    ),
    '--coddy-terminal-glass-secondary': hexToRgba(
      appearance.glassSecondaryColor,
      appearance.glassIntensity,
    ),
    '--coddy-terminal-glass-secondary-soft': hexToRgba(
      appearance.glassSecondaryColor,
      appearance.glassIntensity * 0.6,
    ),
    '--coddy-terminal-font-family': floatingFontFamilyCss(
      appearance.fontFamily,
    ),
    '--coddy-terminal-font-size': `${appearance.fontSizePx}px`,
    '--coddy-terminal-text': appearance.textColor,
    '--coddy-terminal-bold': appearance.boldTextColor,
    '--coddy-terminal-accent': appearance.accentColor,
  } as CSSProperties

  return (
    <main
      className={`floating-terminal-shell aurora-gradient relative flex flex-col overflow-hidden border border-primary/20 ${
        expanded
          ? 'h-screen w-screen rounded-none'
          : 'm-6 h-[calc(100vh-48px)] w-[calc(100vw-48px)] rounded-xl'
      }`}
      style={terminalStyle}
    >
      <header
        data-testid="floating-terminal-header"
        className="floating-terminal-header relative z-[120] flex w-full flex-shrink-0 items-center justify-between border-b border-primary/20 bg-slate-950/60 px-6 py-3 shadow-[0_4px_30px_rgba(0,0,0,0.15)] backdrop-blur-xl"
      >
        <div className="flex items-center gap-3">
          <Icon
            name="terminal"
            className="h-5 w-5 text-primary drop-shadow-[0_0_8px_rgba(0,240,255,0.55)]"
          />
          <span className="font-display text-xl font-semibold uppercase tracking-[0.18em] text-primary drop-shadow-[0_0_10px_rgba(0,240,255,0.55)]">
            CODDY;
          </span>
        </div>

        <div className="flex items-center gap-3">
          <StatusIndicator status={session.status} />
          <ModelSelector
            model={session.selected_model}
            onLoadModels={listProviderModels}
            onSelect={(model) => {
              void selectModel(model, 'Chat')
            }}
          />
          <button
            type="button"
            onClick={() => {
              void openUi('DesktopApp')
            }}
            className="hidden items-center gap-2 rounded-full border border-outline-variant/80 bg-surface-container-high/70 px-3 py-1 font-mono text-xs text-on-surface-variant transition-colors hover:border-primary/50 hover:text-primary sm:flex"
            title="Open desktop mode"
          >
            <Icon name="desktop" className="h-3.5 w-3.5" />
            Desktop
          </button>
          <button
            type="button"
            onClick={() => {
              const mode: ScreenAssistMode = 'ExplainVisibleScreen'
              setPendingScreenAssistMode(mode)
              setConfirmationDismissed(false)
              void captureAndExplain(mode)
            }}
            className="hidden items-center gap-2 rounded-full border border-outline-variant/80 bg-surface-container-high/70 px-3 py-1 font-mono text-xs text-on-surface-variant transition-colors hover:border-primary/50 hover:text-primary md:flex"
            title="Explain visible screen"
          >
            <Icon name="screen" className="h-3.5 w-3.5" />
            Screen
          </button>
          <button
            type="button"
            className="text-on-surface-variant transition-colors hover:text-primary"
            aria-label="Sensors"
            title="Sensors"
          >
            <Icon name="sensors" className="h-5 w-5" />
          </button>
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            className="text-on-surface-variant transition-colors hover:text-primary"
            aria-label="Settings"
            title="Settings"
          >
            <Icon name="settings" className="h-5 w-5" />
          </button>
          <div className="ml-1 flex items-center gap-1">
            <button
              type="button"
              onClick={handleMinimize}
              className="flex h-5 w-5 items-center justify-center rounded-full bg-surface-container-high/70 text-on-surface-variant/70 transition-colors hover:text-on-surface"
              title="Minimize"
              aria-label="Minimize"
            >
              <Icon name="minimize" className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={handleMaximize}
              className="flex h-5 w-5 items-center justify-center rounded-full bg-surface-container-high/70 text-on-surface-variant/70 transition-colors hover:text-on-surface"
              title={expanded ? 'Restore' : 'Maximize'}
              aria-label={expanded ? 'Restore floating terminal' : 'Maximize'}
            >
              <Icon name="maximize" className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={handleClose}
              className="flex h-5 w-5 items-center justify-center rounded-full bg-red-500/10 text-red-300/70 transition-colors hover:text-red-300"
              title="Close"
              aria-label="Close"
            >
              <Icon name="close" className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </header>

      <SelectionCopyRegion
        data-testid="floating-terminal-canvas"
        className="terminal-canvas flex-1 select-text overflow-y-auto px-8 py-7"
      >
        <div className="flex flex-col gap-7">
          <SystemLine text="system.initialize(context='coddy_floating');" />
          <SystemLine text={`daemon.status=${connecting ? 'connecting' : 'ready'}; model='${session.selected_model.name}';`} />
          <SystemLine text="awaiting user command." />

          {toolActivity.length > 0 && (
            <div className="rounded-lg border border-primary/15 bg-surface-container/35 px-4 py-3 font-mono text-xs text-on-surface-variant backdrop-blur-md">
              <div className="mb-2 uppercase tracking-[0.2em] text-primary/80">
                agent.tools
              </div>
              <div className="flex flex-col gap-1">
                {toolActivity.map((activity) => (
                  <div
                    key={activity.id}
                    className="flex min-w-0 items-center justify-between gap-3"
                  >
                    <span className="truncate">{activity.name}</span>
                    <span
                      className={
                        activity.status === 'Running'
                          ? 'text-primary'
                          : activity.status === 'Succeeded'
                            ? 'text-emerald-300'
                            : 'text-red-300'
                      }
                    >
                      {activity.status}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {subagentActivity.length > 0 && (
            <div className="rounded-lg border border-primary/15 bg-surface-container/35 px-4 py-3 font-mono text-xs text-on-surface-variant backdrop-blur-md">
              <div className="mb-2 uppercase tracking-[0.2em] text-primary/80">
                agent.subagents
              </div>
              <div className="flex flex-col gap-1">
                {subagentActivity.map((activity) => (
                  <div
                    key={activity.id}
                    className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-3"
                  >
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate">
                        {activity.name} [{activity.mode}]
                      </span>
                      {activity.required_output_fields.length > 0 && (
                        <span className="truncate text-[10px] uppercase tracking-[0.14em] text-muted">
                          output: {formatRequiredOutputFields(
                            activity.required_output_fields,
                            activity.output_additional_properties_allowed,
                          )}
                        </span>
                      )}
                    </span>
                    <span
                      className={
                        activity.status === 'Running'
                          ? 'text-primary'
                          : activity.status === 'Blocked'
                            || activity.status === 'Failed'
                            ? 'text-red-300'
                            : 'text-emerald-300'
                      }
                    >
                      {activity.status} // {activity.readiness_score}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {historyOpen && (
            <ConversationHistoryPanel
              records={conversationHistory}
              status={conversationHistoryStatus}
              error={conversationHistoryError}
              onSelect={handleOpenConversation}
              onClose={() => setHistoryOpen(false)}
            />
          )}

          {statusOpen && (
            <SessionStatusPanel
              session={session}
              workspacePath={activeWorkspacePath}
              toolCount={toolCatalog?.length ?? 0}
              onClose={() => setStatusOpen(false)}
            />
          )}

          {error && (
            <div className="flex items-center gap-3 rounded-lg border border-red-400/25 bg-red-500/10 px-4 py-3 font-mono text-sm text-red-300">
              <Icon name="alert" className="h-4 w-4" />
              <span className="min-w-0 flex-1 break-words">{error}</span>
              {reconnecting && (
                <button
                  type="button"
                  onClick={reconnect}
                  className="rounded border border-red-300/30 px-2 py-1 text-xs transition-colors hover:bg-red-300/10"
                >
                  retry
                </button>
              )}
            </div>
          )}

          {session.messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}

          {session.status === 'Thinking' && !session.streaming_text && (
            <ThinkingIndicator
              animation={thinkingAnimation}
              label="thinking_response"
            />
          )}

          {session.streaming_text && (
            <div className="flex w-full items-start gap-4">
              <div className="mt-1 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary bg-primary/10 text-primary shadow-[0_0_22px_rgba(0,219,233,0.2)]">
                <Icon name="bot" className="h-4 w-4" />
              </div>
              <div className="min-w-0 flex-1 rounded-lg border border-primary/20 bg-surface-container/45 px-5 py-4">
                <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
                  streaming_response
                </div>
                <MarkdownContent text={session.streaming_text} />
                <span className="streaming-cursor mt-2 inline-block" />
                <p className="mt-3 text-[11px] text-on-surface-muted">
                  Pressione (Esc) para parar.
                </p>
              </div>
            </div>
          )}

          {session.pending_permission && (
            <ToolApprovalPanel
              request={session.pending_permission}
              onReply={(requestId, reply) => {
                void replyPermission(requestId, reply)
              }}
            />
          )}

          <div ref={messagesEndRef} />
        </div>
      </SelectionCopyRegion>

      <div className="floating-terminal-input-row flex flex-shrink-0 items-center gap-3 border-t border-primary/15 bg-surface-dim/70 px-8 py-4 backdrop-blur-md">
        <div className="flex-1">
          <InputBar
            onSend={handleSend}
            disabled={
              connecting
              || session.status === 'Streaming'
              || session.status === 'Thinking'
              || session.status === 'AwaitingToolApproval'
            }
            placeholder={
              session.status === 'AwaitingToolApproval'
                ? 'Tool approval required'
                : 'Enter command or prompt...'
            }
          />
        </div>
        <VoiceButton
          onCapture={handleCaptureVoice}
          onCancel={cancelVoiceCapture}
          disabled={connecting}
        />
      </div>

      {session.status === 'AwaitingConfirmation' && !confirmationDismissed && (
        <AssessmentConfirmModal
          onConfirm={() => {
            const mode = pendingScreenAssistMode ?? 'ExplainVisibleScreen'
            setPendingScreenAssistMode(null)
            void captureAndExplain(mode, 'PermittedAi')
          }}
          onDismiss={() => {
            setPendingScreenAssistMode(null)
            setConfirmationDismissed(true)
            void dismissConfirmation()
          }}
        />
      )}

      {settingsOpen && (
        <FloatingSettingsModal
          value={appearance}
          onChange={handleAppearanceChange}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </main>
  )
}

function formatRequiredOutputFields(
  fields: string[],
  additionalPropertiesAllowed: boolean,
): string {
  const visibleFields = fields.slice(0, 3).join(', ')
  const remaining = fields.length > 3 ? ` +${fields.length - 3}` : ''
  const strictness = additionalPropertiesAllowed ? 'open' : 'strict'
  return `${visibleFields}${remaining} // ${strictness}`
}

function SystemLine({ text }: { text: string }) {
  return (
    <div className="flex gap-3 font-mono text-sm text-on-surface-variant/45">
      <span className="text-primary/60">&gt;</span>
      <span>{text}</span>
    </div>
  )
}

function floatingFontFamilyCss(fontFamily: FloatingAppearanceSettings['fontFamily']): string {
  switch (fontFamily) {
    case 'mono':
      return 'JetBrains Mono, Fira Code, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace'
    case 'serif':
      return 'Georgia, Cambria, "Times New Roman", Times, serif'
    case 'display':
      return 'Manrope, Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    case 'system':
      return 'Inter, Manrope, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
  }
}

function hexToRgba(hex: string, alpha: number): string {
  const normalized = hex.replace('#', '')
  const red = Number.parseInt(normalized.slice(0, 2), 16)
  const green = Number.parseInt(normalized.slice(2, 4), 16)
  const blue = Number.parseInt(normalized.slice(4, 6), 16)
  const opacity = Math.max(0, Math.min(1, alpha))
  return `rgba(${red}, ${green}, ${blue}, ${opacity})`
}
