// main/ipcBridge.ts
// Electron main process: spawns coddy CLI and bridges to renderer via IPC.

import { spawn, ChildProcess } from 'child_process'
import { createInterface } from 'readline'
import * as path from 'path'
import { app, ipcMain, BrowserWindow, screen, safeStorage, dialog } from 'electron'
import type { Rectangle } from 'electron'
import { resolveCoddyBinaryPath } from './coddyBinary'
import { restartCoddyRuntimeProcess } from './runtimeProcess'
import {
  listProviderModels,
  type ModelProviderListPayload,
  type ModelProviderListPayloadResult,
} from './modelProviders'
import {
  SecureCredentialStore,
  type CredentialStorageResult,
  type ProviderCredentialRecord,
} from './secureCredentialStore'
import { buildRuntimeCredentialEnvironment } from './runtimeCredentialBridge'
import { redactSensitiveLogText } from './sensitiveLogRedaction'
import {
  ensureLocalModelReady,
  type LocalModelProviderPreference,
} from './localModelManager'
import { ActiveChildProcessTracker } from './activeChildProcessTracker'
import {
  readJson,
  readJsonWithTimeout,
  terminateChild,
} from './childProcessJson'
import {
  activeWorkspaceEnvironment,
  getActiveWorkspacePath,
  isDirectory,
  normalizeWorkspacePath,
  persistWorkspaceSelection,
} from './workspaceManager'
import { buildCoddySpawnEnv } from './coddySpawnEnv'

type ModelRef = {
  provider: string
  name: string
}

type ModelRole = 'Chat' | 'Ocr' | 'Asr' | 'Tts' | 'Embedding'
type ModelSelectionOptions = {
  localProviderPreference?: LocalModelProviderPreference
}
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
type PermissionReply = 'Once' | 'Always' | 'Reject'
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

type WindowMaximizeResult = {
  maximized: boolean
}

type ReplCommandResult = {
  text?: string
  summary?: string
  message?: string
  error?: { code: string; message: string }
}

type VoiceCapturePayload = {
  speakResponse?: boolean
}

type WorkspaceSelectionResult = {
  path: string | null
  cancelled?: boolean
  message?: string
  error?: { code: string; message: string }
}

type MultiagentEvalPayload = {
  baseline?: string
  writeBaseline?: string
}

const VOICE_CAPTURE_TIMEOUT_MS = 120_000

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function coddySpawn(
  args: string[],
  env: Record<string, string> = {},
  options: { detached?: boolean } = {},
): ChildProcess {
  const electronProcess = process as NodeJS.Process & {
    resourcesPath?: string
  }
  const child = spawn(resolveCoddyBinaryPath({
    appPath: app.getAppPath(),
    env: process.env,
    resourcesPath: electronProcess.resourcesPath,
  }), args, {
    detached: options.detached ?? false,
    env: buildCoddySpawnEnv(process.env, activeWorkspaceEnvironment(), env),
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  child.stderr?.on('data', (chunk: Buffer) => {
    console.error(
      `[coddy stderr] ${redactSensitiveLogText(chunk.toString().trim())}`,
    )
  })

  return child
}

// ---------------------------------------------------------------------------
// Active stream tracking (for reaping on window close)
// ---------------------------------------------------------------------------

const activeStreams = new Map<string, ChildProcess>()
const activeRunCommands = new ActiveChildProcessTracker(terminateChild)
let activeVoiceCapture: ChildProcess | null = null
let voiceCaptureCancelRequested = false
const resizeSessions = new Map<
  number,
  {
    window: BrowserWindow
    edge: ResizeEdge
    startX: number
    startY: number
    bounds: Rectangle
    minWidth: number
    minHeight: number
    timer: ReturnType<typeof setInterval>
  }
>()
const restoreBoundsByWebContents = new Map<number, Rectangle>()

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
  const credentialStore = createCredentialStore()

  // ---- Window controls ----
  ipcMain.handle('window:close', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  ipcMain.handle('window:minimize', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  ipcMain.handle('window:maximize', (event) => {
    const targetWindow = BrowserWindow.fromWebContents(event.sender)
    if (!targetWindow) return { maximized: false }
    return toggleWindowMaximize(event.sender.id, targetWindow)
  })

  ipcMain.handle(
    'window:resize-start',
    (event, payload: ResizeStartPayload) => {
      const targetWindow = BrowserWindow.fromWebContents(event.sender)
      if (!targetWindow) return { ok: false }

      const [minWidth, minHeight] = targetWindow.getMinimumSize()
      clearResizeSession(event.sender.id)
      restoreBoundsByWebContents.delete(event.sender.id)

      const session = {
        window: targetWindow,
        edge: payload.edge,
        startX: payload.screenX,
        startY: payload.screenY,
        bounds: targetWindow.getBounds(),
        minWidth: minWidth ?? 680,
        minHeight: minHeight ?? 400,
        timer: setInterval(() => {
          const active = resizeSessions.get(event.sender.id)
          if (!active) return
          if (active.window.isDestroyed()) {
            clearResizeSession(event.sender.id)
            return
          }
          const point = screen.getCursorScreenPoint()
          updateResizeSession(active, { screenX: point.x, screenY: point.y })
        }, 16),
      }

      resizeSessions.set(event.sender.id, {
        ...session,
      })
      return { ok: true }
    },
  )

  ipcMain.handle('window:resize-drag', (event, payload: ResizeDragPayload) => {
    const targetWindow = BrowserWindow.fromWebContents(event.sender)
    const session = resizeSessions.get(event.sender.id)
    if (!targetWindow || !session) return { ok: false }

    updateResizeSession(session, payload)
    return { ok: true }
  })

  ipcMain.handle('window:resize-end', (event) => {
    clearResizeSession(event.sender.id)
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

  ipcMain.handle('repl:history', async (_event, limit?: number) => {
    const args = ['session', 'history']
    if (Number.isSafeInteger(limit) && Number(limit) > 0) {
      args.push('--limit', String(limit))
    }
    return readJson(coddySpawn(args))
  })

  // ---- Workspace selection ----
  ipcMain.handle('workspace:get-active', async (): Promise<WorkspaceSelectionResult> => {
    return { path: getActiveWorkspacePath() }
  })

  ipcMain.handle('workspace:select-folder', async (event): Promise<WorkspaceSelectionResult> => {
    const targetWindow = BrowserWindow.fromWebContents(event.sender)
    const result = targetWindow
      ? await dialog.showOpenDialog(targetWindow, {
          title: 'Select Coddy workspace',
          properties: ['openDirectory'],
        })
      : await dialog.showOpenDialog({
          title: 'Select Coddy workspace',
          properties: ['openDirectory'],
        })

    if (result.canceled || result.filePaths.length === 0) {
      return { path: getActiveWorkspacePath(), cancelled: true }
    }

    const selectedPath = normalizeWorkspacePath(result.filePaths[0])
    if (!selectedPath || !isDirectory(selectedPath)) {
      return {
        path: getActiveWorkspacePath(),
        error: {
          code: 'WORKSPACE_INVALID_PATH',
          message: 'Coddy needs a readable directory as the active workspace.',
        },
      }
    }

    persistWorkspaceSelection(app.getPath('userData'), selectedPath)
    restartRuntimeForActiveWorkspace()
    return {
      path: selectedPath,
      message: `Coddy workspace set to ${selectedPath}.`,
    }
  })

  // ---- Provider model catalogs ----
  ipcMain.handle(
    'models:list',
    async (_event, payload: ModelProviderListPayload) => {
      return listProviderModelsWithSecureCredentials(payload, credentialStore)
    },
  )

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
    const credentialEnv = await runtimeCredentialEnvironmentForActiveModel(
      credentialStore,
    )
    return runActiveCoddyCommand(['ask', text], credentialEnv)
  })

  ipcMain.handle(
    'repl:eval-multiagent',
    async (_event, payload: MultiagentEvalPayload = {}) => {
      return runMultiagentEval(payload)
    },
  )

  ipcMain.handle('repl:eval-prompt-battery', async () => {
    return runPromptBatteryEval()
  })

  // ---- Voice: capture + transcribe via coddy CLI ----
  ipcMain.handle('voice:capture', async (_event, payload?: VoiceCapturePayload) => {
    if (activeVoiceCapture) {
      return {
        error: {
          code: 'VOICE_CAPTURE_IN_PROGRESS',
          message: 'voice capture is already running',
        },
      }
    }

    const credentialEnv = await runtimeCredentialEnvironmentForActiveModel(
      credentialStore,
    )
    const args = voiceCaptureArgs(payload)
    const child = coddySpawn(
      args,
      credentialEnv,
      { detached: true },
    )
    activeVoiceCapture = child
    voiceCaptureCancelRequested = false

    try {
      const raw = await readJsonWithTimeout(
        child,
        VOICE_CAPTURE_TIMEOUT_MS,
        'voice capture timed out',
      )
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
      return normalizeVoiceCaptureFailure(err)
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
      terminateChild(activeVoiceCapture)
    }
    return { ok: true }
  })

  ipcMain.handle('repl:voice-turn', async (_event, transcript: string) => {
    const credentialEnv = await runtimeCredentialEnvironmentForActiveModel(
      credentialStore,
    )
    return runActiveCoddyCommand(
      ['voice', '--transcript', transcript],
      credentialEnv,
    )
  })

  ipcMain.handle('repl:stop-speaking', async () => {
    const child = coddySpawn(['stop-speaking'])
    await readJson(child)
    return { ok: true }
  })

  ipcMain.handle('repl:stop-active-run', async () => {
    activeRunCommands.terminateActive()
    const child = coddySpawn(['stop-active-run'])
    await readJson(child)
    return { ok: true }
  })

  ipcMain.handle('repl:new-session', async () => {
    activeRunCommands.terminateActive()
    return runCoddyCommand(['session', 'new'])
  })

  ipcMain.handle('repl:open-conversation', async (_event, sessionId: string) => {
    activeRunCommands.terminateActive()
    return runCoddyCommand(['session', 'open', '--id', sessionId])
  })

  ipcMain.handle(
    'repl:select-model',
    async (
      _event,
      model: ModelRef,
      role: ModelRole,
      options?: ModelSelectionOptions,
    ) => {
      const localModelReady = await ensureLocalModelReady(model, {
        preferredProvider: normalizeLocalProviderPreference(
          options?.localProviderPreference,
        ),
      })
      if (localModelReady.status === 'error') {
        return {
          error: {
            code: localModelReady.code ?? 'LOCAL_MODEL_PRELOAD_FAILED',
            message: localModelReady.message,
          },
        }
      }
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
      return runActiveCoddyCommand([
        'screen',
        'explain',
        '--mode',
        toCliScreenAssistMode(mode),
        '--policy',
        toCliAssessmentPolicy(policy),
      ])
    },
  )

  ipcMain.handle('repl:dismiss-confirmation', async () => {
    return runCoddyCommand(['screen', 'dismiss-confirmation'])
  })

  ipcMain.handle(
    'repl:permission-reply',
    async (_event, requestId: string, reply: string) => {
      return runCoddyCommand([
        'permission',
        'reply',
        '--request-id',
        requestId,
        '--reply',
        toCliPermissionReply(reply),
      ])
    },
  )
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
    terminateChild(activeVoiceCapture)
  }
  activeRunCommands.terminateActive()
  activeRunCommands.clear()
  activeVoiceCapture = null
  voiceCaptureCancelRequested = false
  for (const webContentsId of Array.from(resizeSessions.keys())) {
    clearResizeSession(webContentsId)
  }
  restoreBoundsByWebContents.clear()
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

function createCredentialStore(): SecureCredentialStore {
  return new SecureCredentialStore({
    filePath: path.join(app.getPath('userData'), 'secure-model-credentials.json'),
    isEncryptionAvailable: () => safeStorage.isEncryptionAvailable(),
    encryptString: (value) => safeStorage.encryptString(value),
    decryptString: (value) => safeStorage.decryptString(value),
  })
}

async function listProviderModelsWithSecureCredentials(
  payload: ModelProviderListPayload,
  credentialStore: SecureCredentialStore,
): Promise<ModelProviderListPayloadResult> {
  const stored = await credentialStore.get(payload.provider)
  const apiKey = modelListCredential(payload, stored)
  const endpoint = payload.endpoint?.trim() || stored?.endpoint
  const apiVersion = payload.apiVersion?.trim() || stored?.apiVersion
  const request: ModelProviderListPayload = {
    provider: payload.provider,
    ...(apiKey ? { apiKey } : {}),
    ...(endpoint ? { endpoint } : {}),
    ...(apiVersion ? { apiVersion } : {}),
  }

  const result = await listProviderModels(request)
  if (result.error) return result

  const storageRecord = getCredentialRecordToPersist(
    payload,
    stored,
    endpoint,
    apiVersion,
  )
  if (!payload.rememberCredential || !storageRecord) return result

  return {
    ...result,
    credentialStorage: await saveCredentialRecord(
      credentialStore,
      payload.provider,
      storageRecord,
    ),
  }
}

function modelListCredential(
  payload: ModelProviderListPayload,
  stored: ProviderCredentialRecord | null,
): string | undefined {
  const supplied = payload.apiKey?.trim()
  if (supplied) return supplied

  const storedToken = stored?.apiKey?.trim()
  if (!storedToken) return undefined

  if (payload.provider === 'vertex') {
    return isGoogleOAuthCredential(storedToken) ? storedToken : undefined
  }

  return storedToken
}

function isGoogleOAuthCredential(value: string): boolean {
  return /^Bearer\s+/i.test(value) || value.startsWith('ya29.')
}

function getCredentialRecordToPersist(
  payload: ModelProviderListPayload,
  stored: ProviderCredentialRecord | null,
  endpoint: string | undefined,
  apiVersion: string | undefined,
): ProviderCredentialRecord | null {
  const apiKey = payload.apiKey?.trim() || stored?.apiKey
  if (!apiKey) return null
  return {
    apiKey,
    ...(endpoint ? { endpoint } : {}),
    ...(apiVersion ? { apiVersion } : {}),
  }
}

async function saveCredentialRecord(
  credentialStore: SecureCredentialStore,
  provider: ModelProviderListPayload['provider'],
  record: ProviderCredentialRecord,
): Promise<CredentialStorageResult> {
  try {
    return await credentialStore.save(provider, record)
  } catch {
    return {
      persisted: false,
      message: 'Secure credential storage failed; token was not saved.',
    }
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

export function voiceCaptureArgs(payload?: VoiceCapturePayload): string[] {
  return payload?.speakResponse
    ? ['--speak', 'voice', '--overlay']
    : ['voice', '--overlay']
}

function normalizeVoiceCaptureFailure(error: unknown): ReplCommandResult {
  const message = error instanceof Error ? error.message : String(error)
  if (/coddy-voice\.lock|voice shortcut is already active|File exists/i.test(message)) {
    return {
      error: {
        code: 'VOICE_LOCK_ACTIVE',
        message:
          'Coddy voice capture is already active, or the previous recording left a stale lock. Try again; Coddy now clears stale locks automatically when the owning process is gone.',
      },
    }
  }
  return { error: { code: 'VOICE_CAPTURE_FAILED', message } }
}

async function runCoddyCommand(
  args: string[],
  env: Record<string, string> = {},
): Promise<ReplCommandResult> {
  return runCoddyCommandFromChild(coddySpawn(args, env))
}

async function runActiveCoddyCommand(
  args: string[],
  env: Record<string, string> = {},
): Promise<ReplCommandResult> {
  const child = coddySpawn(args, env, { detached: true })
  return activeRunCommands.track(child, async (trackedChild) => {
    const result = await runCoddyCommandFromChild(trackedChild)
    if (result.error && activeRunCommands.wasTerminated(trackedChild)) {
      return {
        message: 'Active run cancellation requested.',
      }
    }
    return result
  })
}

async function runMultiagentEval(payload: MultiagentEvalPayload): Promise<unknown> {
  const args = ['eval', 'multiagent', '--json']
  const baseline = normalizeOptionalPath(payload.baseline)
  const writeBaseline = normalizeOptionalPath(payload.writeBaseline)
  if (baseline) {
    args.push('--baseline', baseline)
  }
  if (writeBaseline) {
    args.push('--write-baseline', writeBaseline)
  }

  return readJson(coddySpawn(args))
}

async function runPromptBatteryEval(): Promise<unknown> {
  return readJson(coddySpawn(['eval', 'prompt-battery', '--json']))
}

function restartRuntimeForActiveWorkspace(): void {
  cleanupStreams()
  const electronProcess = process as NodeJS.Process & {
    resourcesPath?: string
  }
  restartCoddyRuntimeProcess({
    appPath: app.getAppPath(),
    env: {
      ...process.env,
      ...activeWorkspaceEnvironment(),
    },
    resourcesPath: electronProcess.resourcesPath,
  })
}

function normalizeOptionalPath(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function normalizeLocalProviderPreference(
  value: unknown,
): LocalModelProviderPreference {
  return value === 'ollama' || value === 'hf' || value === 'vllm'
    ? value
    : 'auto'
}

async function runtimeCredentialEnvironmentForActiveModel(
  credentialStore: SecureCredentialStore,
): Promise<Record<string, string>> {
  const snapshot = await readJson(coddySpawn(['session', 'snapshot']))
  const selectedModel = selectedModelFromSnapshot(snapshot)
  if (!selectedModel) return {}
  return buildRuntimeCredentialEnvironment(selectedModel, credentialStore)
}

function selectedModelFromSnapshot(snapshot: unknown): ModelRef | null {
  if (!snapshot || typeof snapshot !== 'object') return null

  const session = (snapshot as { session?: unknown }).session
  if (!session || typeof session !== 'object') return null

  const selectedModel = (session as { selected_model?: unknown }).selected_model
  if (!selectedModel || typeof selectedModel !== 'object') return null

  const model = selectedModel as Partial<ModelRef>
  if (typeof model.provider !== 'string' || typeof model.name !== 'string') {
    return null
  }
  return {
    provider: model.provider,
    name: model.name,
  }
}

function toggleWindowMaximize(
  webContentsId: number,
  targetWindow: BrowserWindow,
): WindowMaximizeResult {
  clearResizeSession(webContentsId)

  const restoreBounds = restoreBoundsByWebContents.get(webContentsId)
  if (restoreBounds) {
    if (targetWindow.isMaximized()) {
      targetWindow.unmaximize()
    }
    targetWindow.setBounds(restoreBounds, false)
    restoreBoundsByWebContents.delete(webContentsId)
    return { maximized: false }
  }

  if (targetWindow.isMaximized()) {
    targetWindow.unmaximize()
    return { maximized: false }
  }

  const currentBounds = targetWindow.getBounds()
  const display = screen.getDisplayMatching(currentBounds)

  restoreBoundsByWebContents.set(webContentsId, currentBounds)
  targetWindow.setBounds(display.workArea, false)

  return { maximized: true }
}

function clearResizeSession(webContentsId: number): void {
  const session = resizeSessions.get(webContentsId)
  if (session) {
    clearInterval(session.timer)
  }
  resizeSessions.delete(webContentsId)
}

function updateResizeSession(
  session: {
    window: BrowserWindow
    edge: ResizeEdge
    startX: number
    startY: number
    bounds: Rectangle
    minWidth: number
    minHeight: number
  },
  point: ResizeDragPayload,
): void {
  const dx = point.screenX - session.startX
  const dy = point.screenY - session.startY
  session.window.setBounds(resizeBounds(session, dx, dy), false)
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

function toCliPermissionReply(reply: string): string {
  const normalized = reply as PermissionReply
  switch (normalized) {
    case 'Once':
      return 'once'
    case 'Always':
      return 'always'
    case 'Reject':
      return 'reject'
    default:
      return 'reject'
  }
}
