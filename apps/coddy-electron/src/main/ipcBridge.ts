// main/ipcBridge.ts
// Electron main process: spawns coddy CLI and bridges to renderer via IPC.

import { spawn, ChildProcess } from 'child_process'
import { createInterface } from 'readline'
import { ipcMain, BrowserWindow } from 'electron'
import type { Rectangle } from 'electron'

const CODDY_BIN = process.env.CODDY_BIN || 'coddy'

type ModelRef = {
  provider: string
  name: string
}

type ModelRole = 'Chat' | 'Ocr' | 'Asr' | 'Tts' | 'Embedding'
type ReplMode = 'FloatingTerminal' | 'DesktopApp'
type ScreenAssistMode =
  | 'ExplainVisibleScreen'
  | 'ExplainCode'
  | 'DebugError'
  | 'MultipleChoice'
  | 'SummarizeDocument'
type AssessmentPolicy =
  | 'Practice'
  | 'PermittedAi'
  | 'SyntaxOnly'
  | 'RestrictedAssessment'
  | 'UnknownAssessment'
type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw'

type ResizeStartPayload = {
  edge: ResizeEdge
  screenX: number
  screenY: number
}

type ResizeDragPayload = {
  screenX: number
  screenY: number
}

type ReplCommandResult = {
  text?: string
  summary?: string
  message?: string
  error?: { code: string; message: string }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function coddySpawn(args: string[]): ChildProcess {
  const child = spawn(CODDY_BIN, args, {
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  child.stderr?.on('data', (chunk: Buffer) => {
    console.error(`[coddy stderr] ${chunk.toString().trim()}`)
  })

  return child
}

async function readJson(child: ChildProcess): Promise<unknown> {
  return new Promise((resolve, reject) => {
    let stdout = ''
    let stderr = ''
    child.stdout?.on('data', (chunk: Buffer) => { stdout += chunk.toString() })
    child.stderr?.on('data', (chunk: Buffer) => { stderr += chunk.toString() })

    child.on('close', (code) => {
      if (code !== 0) {
        const detail = stderr.trim()
        reject(new Error(
          detail ? `coddy exited ${code}: ${detail}` : `coddy exited ${code}`,
        ))
        return
      }
      try {
        resolve(JSON.parse(stdout.trim()))
      } catch {
        resolve(stdout.trim())
      }
    })

    child.on('error', reject)
  })
}

// ---------------------------------------------------------------------------
// Active stream tracking (for reaping on window close)
// ---------------------------------------------------------------------------

const activeStreams = new Map<string, ChildProcess>()
let activeVoiceCapture: ChildProcess | null = null
let voiceCaptureCancelRequested = false
const resizeSessions = new Map<
  number,
  {
    edge: ResizeEdge
    startX: number
    startY: number
    bounds: Rectangle
    minWidth: number
    minHeight: number
  }
>()

function reapStream(streamId: string): void {
  const child = activeStreams.get(streamId)
  if (child) {
    child.kill()
    activeStreams.delete(streamId)
  }
}

// ---------------------------------------------------------------------------
// IPC Handler registration
// ---------------------------------------------------------------------------

export function registerIpcHandlers(): void {
  // ---- Window controls ----
  ipcMain.handle('window:close', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  ipcMain.handle('window:minimize', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  ipcMain.handle('window:maximize', (event) => {
    const targetWindow = BrowserWindow.fromWebContents(event.sender)
    if (!targetWindow) return
    if (targetWindow.isMaximized()) {
      targetWindow.unmaximize()
      return
    }
    targetWindow.maximize()
  })

  ipcMain.handle(
    'window:resize-start',
    (event, payload: ResizeStartPayload) => {
      const targetWindow = BrowserWindow.fromWebContents(event.sender)
      if (!targetWindow) return { ok: false }

      const [minWidth, minHeight] = targetWindow.getMinimumSize()
      resizeSessions.set(event.sender.id, {
        edge: payload.edge,
        startX: payload.screenX,
        startY: payload.screenY,
        bounds: targetWindow.getBounds(),
        minWidth: minWidth ?? 680,
        minHeight: minHeight ?? 400,
      })
      return { ok: true }
    },
  )

  ipcMain.handle('window:resize-drag', (event, payload: ResizeDragPayload) => {
    const targetWindow = BrowserWindow.fromWebContents(event.sender)
    const session = resizeSessions.get(event.sender.id)
    if (!targetWindow || !session) return { ok: false }

    const dx = payload.screenX - session.startX
    const dy = payload.screenY - session.startY
    const next = resizeBounds(session, dx, dy)

    targetWindow.setBounds(next, false)
    return { ok: true }
  })

  ipcMain.handle('window:resize-end', (event) => {
    resizeSessions.delete(event.sender.id)
    return { ok: true }
  })

  // ---- Snapshot ----
  ipcMain.handle('repl:snapshot', async () => {
    return readJson(coddySpawn(['session', 'snapshot']))
  })

  // ---- Incremental events ----
  ipcMain.handle('repl:events-after', async (_event, afterSequence: number) => {
    return readJson(
      coddySpawn(['session', 'events', '--after', String(afterSequence)]),
    )
  })

  // ---- Tool catalog ----
  ipcMain.handle('repl:tools', async () => {
    return readJson(coddySpawn(['session', 'tools']))
  })

  // ---- Watch (streaming) ----
  ipcMain.handle('repl:watch-start', async (_event, afterSequence: number) => {
    const streamId = String(Math.random()).slice(2, 10)
    const child = coddySpawn([
      'session', 'watch', '--after', String(afterSequence),
    ])

    activeStreams.set(streamId, child)
    void pumpWatchStream(streamId, child)

    return { streamId }
  })

  ipcMain.handle('repl:watch-close', async (_event, streamId: string) => {
    reapStream(streamId)
  })

  // ---- Commands ----
  ipcMain.handle('repl:ask', async (_event, text: string) => {
    return runCoddyCommand(['ask', text])
  })

  // ---- Voice: capture + transcribe via coddy CLI ----
  ipcMain.handle('voice:capture', async () => {
    if (activeVoiceCapture) {
      return {
        error: {
          code: 'VOICE_CAPTURE_IN_PROGRESS',
          message: 'voice capture is already running',
        },
      }
    }

    const child = coddySpawn(['voice', '--overlay'])
    activeVoiceCapture = child
    voiceCaptureCancelRequested = false

    try {
      const raw = await readJson(child)
      return normalizeCommandResult(raw)
    } catch (err) {
      if (voiceCaptureCancelRequested) {
        return {
          error: {
            code: 'VOICE_CAPTURE_CANCELLED',
            message: 'voice capture cancelled',
          },
        }
      }
      return { error: { code: 'VOICE_CAPTURE_FAILED', message: String(err) } }
    } finally {
      if (activeVoiceCapture === child) {
        activeVoiceCapture = null
      }
      voiceCaptureCancelRequested = false
    }
  })

  ipcMain.handle('voice:capture-cancel', async () => {
    if (activeVoiceCapture && !activeVoiceCapture.killed) {
      voiceCaptureCancelRequested = true
      activeVoiceCapture.kill()
    }
    return { ok: true }
  })

  ipcMain.handle('repl:voice-turn', async (_event, transcript: string) => {
    return runCoddyCommand(['voice', '--transcript', transcript])
  })

  ipcMain.handle('repl:stop-speaking', async () => {
    const child = coddySpawn(['stop-speaking'])
    await readJson(child)
    return { ok: true }
  })

  ipcMain.handle('repl:stop-active-run', async () => {
    const child = coddySpawn(['stop-active-run'])
    await readJson(child)
    return { ok: true }
  })

  ipcMain.handle(
    'repl:select-model',
    async (_event, model: ModelRef, role: ModelRole) => {
      const child = coddySpawn([
        'model',
        'select',
        '--provider',
        model.provider,
        '--name',
        model.name,
        '--role',
        toCliModelRole(role),
      ])
      return runCoddyCommandFromChild(child)
    },
  )

  ipcMain.handle('repl:open-ui', async (_event, mode: ReplMode) => {
    return runCoddyCommand(['ui', 'open', '--mode', toCliReplMode(mode)])
  })

  ipcMain.handle(
    'repl:capture-and-explain',
    async (_event, mode: ScreenAssistMode, policy: AssessmentPolicy) => {
      const child = coddySpawn([
        'screen',
        'explain',
        '--mode',
        toCliScreenAssistMode(mode),
        '--policy',
        toCliAssessmentPolicy(policy),
      ])
      return runCoddyCommandFromChild(child)
    },
  )

  ipcMain.handle('repl:dismiss-confirmation', async () => {
    return runCoddyCommand(['screen', 'dismiss-confirmation'])
  })
}

// ---------------------------------------------------------------------------
// Cleanup on quit
// ---------------------------------------------------------------------------

export function cleanupStreams(): void {
  for (const [, child] of activeStreams) {
    child.kill()
  }
  activeStreams.clear()
  if (activeVoiceCapture && !activeVoiceCapture.killed) {
    activeVoiceCapture.kill()
  }
  activeVoiceCapture = null
  voiceCaptureCancelRequested = false
  resizeSessions.clear()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function pumpWatchStream(streamId: string, child: ChildProcess): Promise<void> {
  try {
    const stdout = child.stdout
    if (!stdout) return

    const rl = createInterface({ input: stdout, crlfDelay: Infinity })
    for await (const line of rl) {
      try {
        const parsed = JSON.parse(line)
        for (const win of BrowserWindow.getAllWindows()) {
          win.webContents.send('repl:watch-event', { streamId, ...parsed })
        }
      } catch {
        // non-JSON line - ignore daemon logs or progress text
      }
    }
  } finally {
    for (const win of BrowserWindow.getAllWindows()) {
      win.webContents.send('repl:watch-event', { streamId, done: true })
    }
    activeStreams.delete(streamId)
  }
}

function normalizeCommandResult(raw: unknown): ReplCommandResult {
  if (typeof raw === 'string') return { text: raw }
  if (raw && typeof raw === 'object') {
    const obj = raw as Record<string, unknown>
    if ('error' in obj || 'Error' in obj) {
      const err = (obj.error ?? obj.Error) as { code?: string; message?: string } | undefined
      return { error: { code: err?.code ?? 'UNKNOWN', message: err?.message ?? String(raw) } }
    }
    if ('summary' in obj) return { text: obj.text as string, summary: obj.summary as string }
    if ('text' in obj) return { text: obj.text as string }
    return { text: JSON.stringify(raw) }
  }
  return { text: String(raw) }
}

async function runCoddyCommand(args: string[]): Promise<ReplCommandResult> {
  return runCoddyCommandFromChild(coddySpawn(args))
}

function resizeBounds(
  session: {
    edge: ResizeEdge
    bounds: Rectangle
    minWidth: number
    minHeight: number
  },
  dx: number,
  dy: number,
): Rectangle {
  const next = { ...session.bounds }
  const edge = session.edge

  if (edge.includes('e')) {
    next.width = session.bounds.width + dx
  }
  if (edge.includes('s')) {
    next.height = session.bounds.height + dy
  }
  if (edge.includes('w')) {
    next.x = session.bounds.x + dx
    next.width = session.bounds.width - dx
  }
  if (edge.includes('n')) {
    next.y = session.bounds.y + dy
    next.height = session.bounds.height - dy
  }

  if (next.width < session.minWidth) {
    if (edge.includes('w')) {
      next.x = session.bounds.x + session.bounds.width - session.minWidth
    }
    next.width = session.minWidth
  }

  if (next.height < session.minHeight) {
    if (edge.includes('n')) {
      next.y = session.bounds.y + session.bounds.height - session.minHeight
    }
    next.height = session.minHeight
  }

  return {
    x: Math.round(next.x),
    y: Math.round(next.y),
    width: Math.round(next.width),
    height: Math.round(next.height),
  }
}

async function runCoddyCommandFromChild(child: ChildProcess): Promise<ReplCommandResult> {
  try {
    const raw = await readJson(child)
    return normalizeCommandResult(raw)
  } catch (err) {
    return {
      error: {
        code: 'CODDY_COMMAND_FAILED',
        message: err instanceof Error ? err.message : String(err),
      },
    }
  }
}

function toCliModelRole(role: ModelRole): string {
  switch (role) {
    case 'Chat':
      return 'chat'
    case 'Ocr':
      return 'ocr'
    case 'Asr':
      return 'asr'
    case 'Tts':
      return 'tts'
    case 'Embedding':
      return 'embedding'
  }
}

function toCliReplMode(mode: ReplMode): string {
  switch (mode) {
    case 'FloatingTerminal':
      return 'floating-terminal'
    case 'DesktopApp':
      return 'desktop-app'
  }
}

function toCliScreenAssistMode(mode: ScreenAssistMode): string {
  switch (mode) {
    case 'ExplainVisibleScreen':
      return 'explain-visible-screen'
    case 'ExplainCode':
      return 'explain-code'
    case 'DebugError':
      return 'debug-error'
    case 'MultipleChoice':
      return 'multiple-choice'
    case 'SummarizeDocument':
      return 'summarize-document'
  }
}

function toCliAssessmentPolicy(policy: AssessmentPolicy): string {
  switch (policy) {
    case 'Practice':
      return 'practice'
    case 'PermittedAi':
      return 'permitted-ai'
    case 'SyntaxOnly':
      return 'syntax-only'
    case 'RestrictedAssessment':
      return 'restricted-assessment'
    case 'UnknownAssessment':
      return 'unknown-assessment'
  }
}
