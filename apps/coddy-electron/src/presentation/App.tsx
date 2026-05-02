// presentation/App.tsx
// Root component: wraps in SessionProvider + ModeProvider.
// AppInner switches between FloatingTerminal and DesktopApp based on mode.

import { useEffect } from 'react'
import { FloatingTerminal } from './views/FloatingTerminal'
import { DesktopApp } from './views/DesktopApp'
import { SessionProvider, useSessionContext, ModeProvider, useMode } from './hooks'
import { WindowResizeHandles } from './components'
import {
  cancelBrowserSpeech,
  isBrowserSpeechActive,
} from './services/voiceSpeech'

/** Inner component that has access to session + mode contexts */
function AppInner() {
  const {
    session,
    connecting,
    cancelRun,
    cancelSpeech,
    cancelVoiceCapture,
  } = useSessionContext()
  const { mode, setMode } = useMode()

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      const editableTarget = isEditableEscapeTarget(e.target)

      if (session.voice.speaking || isBrowserSpeechActive()) {
        e.preventDefault()
        cancelBrowserSpeech()
        void cancelSpeech()
        return
      }

      if (session.status === 'Listening' || session.status === 'Transcribing') {
        e.preventDefault()
        void cancelVoiceCapture()
        return
      }

      if (runCanBeCancelled(session)) {
        e.preventDefault()
        void cancelRun()
        return
      }

      if (editableTarget) return

      if (mode === 'FloatingTerminal') {
        if (typeof window !== 'undefined' && window.replApi) {
          void window.replApi.invoke('window:close')
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [
    cancelRun,
    cancelSpeech,
    cancelVoiceCapture,
    mode,
    session.active_run,
    session.status,
    session.tool_activity,
    session.voice.speaking,
  ])

  useEffect(() => {
    if (!connecting && session.mode !== mode) {
      setMode(session.mode)
    }
  }, [connecting, mode, session.mode, setMode])

  if (connecting) {
    return (
      <>
        <WindowResizeHandles />
        <FloatingTerminal />
      </>
    )
  }

  const activeMode = session.mode

  return (
    <>
      <WindowResizeHandles />
      {activeMode === 'DesktopApp' ? <DesktopApp /> : <FloatingTerminal />}
    </>
  )
}

function runCanBeCancelled(session: ReturnType<typeof useSessionContext>['session']): boolean {
  return (
    Boolean(session.active_run)
    || session.status === 'Thinking'
    || session.status === 'Streaming'
    || session.status === 'BuildingContext'
    || session.status === 'CapturingScreen'
    || session.status === 'AwaitingToolApproval'
    || session.tool_activity.some((activity) => activity.status === 'Running')
  )
}

function isEditableEscapeTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false
  if (target instanceof HTMLInputElement) return true
  if (target instanceof HTMLTextAreaElement) return true
  if (target instanceof HTMLSelectElement) return true
  return target instanceof HTMLElement && target.isContentEditable
}

/** Root provider wrapper */
export function App() {
  return (
    <SessionProvider>
      <ModeProvider>
        <AppInner />
      </ModeProvider>
    </SessionProvider>
  )
}
