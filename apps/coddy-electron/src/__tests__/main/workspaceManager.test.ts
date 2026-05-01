import * as fs from 'fs'
import * as os from 'os'
import * as path from 'path'
import { afterEach, describe, expect, it } from 'vitest'
import {
  activeWorkspaceEnvironment,
  getActiveWorkspacePath,
  loadPersistedWorkspaceSelection,
  persistWorkspaceSelection,
  setActiveWorkspacePath,
  workspaceSelectionFilePath,
} from '../../main/workspaceManager'

const tempRoots: string[] = []

function tempDir(name: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `coddy-${name}-`))
  tempRoots.push(dir)
  return dir
}

afterEach(() => {
  setActiveWorkspacePath(null)
  for (const root of tempRoots.splice(0)) {
    fs.rmSync(root, { recursive: true, force: true })
  }
})

describe('workspaceManager', () => {
  it('persists and reloads a selected workspace directory', () => {
    const userData = tempDir('user-data')
    const workspace = tempDir('workspace')

    persistWorkspaceSelection(userData, workspace)
    setActiveWorkspacePath(null)
    const loaded = loadPersistedWorkspaceSelection(userData)

    expect(loaded.path).toBe(path.resolve(workspace))
    expect(getActiveWorkspacePath()).toBe(path.resolve(workspace))
    expect(activeWorkspaceEnvironment()).toEqual({
      CODDY_WORKSPACE: path.resolve(workspace),
    })
  })

  it('ignores a persisted workspace that no longer exists', () => {
    const userData = tempDir('user-data')
    const missing = path.join(userData, 'missing')
    fs.writeFileSync(
      workspaceSelectionFilePath(userData),
      JSON.stringify({ path: missing }),
    )

    expect(loadPersistedWorkspaceSelection(userData)).toEqual({ path: null })
    expect(activeWorkspaceEnvironment()).toEqual({})
  })
})
