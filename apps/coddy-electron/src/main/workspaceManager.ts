import * as fs from 'fs'
import * as path from 'path'

const WORKSPACE_SELECTION_FILE = 'workspace-selection.json'

let activeWorkspacePath: string | null = null

export type WorkspaceSelectionState = {
  path: string | null
}

export function getActiveWorkspacePath(): string | null {
  return activeWorkspacePath
}

export function setActiveWorkspacePath(workspacePath: string | null): void {
  activeWorkspacePath = normalizeWorkspacePath(workspacePath)
}

export function activeWorkspaceEnvironment(): Record<string, string> {
  return activeWorkspacePath ? { CODDY_WORKSPACE: activeWorkspacePath } : {}
}

export function workspaceSelectionFilePath(userDataPath: string): string {
  return path.join(userDataPath, WORKSPACE_SELECTION_FILE)
}

export function loadPersistedWorkspaceSelection(
  userDataPath: string,
): WorkspaceSelectionState {
  const filePath = workspaceSelectionFilePath(userDataPath)
  try {
    const raw = fs.readFileSync(filePath, 'utf8')
    const parsed = JSON.parse(raw) as Partial<WorkspaceSelectionState>
    const workspacePath = normalizeWorkspacePath(parsed.path)
    if (!workspacePath || !isDirectory(workspacePath)) {
      return { path: null }
    }
    activeWorkspacePath = workspacePath
    return { path: workspacePath }
  } catch {
    return { path: null }
  }
}

export function persistWorkspaceSelection(
  userDataPath: string,
  workspacePath: string | null,
): WorkspaceSelectionState {
  const normalized = normalizeWorkspacePath(workspacePath)
  fs.mkdirSync(userDataPath, { recursive: true })
  fs.writeFileSync(
    workspaceSelectionFilePath(userDataPath),
    `${JSON.stringify({ path: normalized }, null, 2)}\n`,
    'utf8',
  )
  activeWorkspacePath = normalized
  return { path: normalized }
}

export function normalizeWorkspacePath(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  return trimmed.length > 0 ? path.resolve(trimmed) : null
}

export function isDirectory(value: string): boolean {
  try {
    return fs.statSync(value).isDirectory()
  } catch {
    return false
  }
}
