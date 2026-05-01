import type { DesktopTab } from '@/presentation/components/Sidebar'

export type UiSlashCommand =
  | { kind: 'open-settings' }
  | { kind: 'open-desktop-tab'; tab: DesktopTab }

const DESKTOP_TAB_STORAGE_KEY = 'coddy:desktop-active-tab'

const TAB_COMMANDS: Record<string, DesktopTab> = {
  chat: 'chat',
  workspace: 'workspace',
  workspaces: 'workspace',
  files: 'workspace',
  models: 'models',
  model: 'models',
}

const SETTINGS_COMMANDS = new Set(['settings', 'setting', 'settins', 'config'])

export function resolveUiSlashCommand(input: string): UiSlashCommand | null {
  const trimmed = input.trim()
  if (!trimmed.startsWith('/')) return null

  const command = trimmed.slice(1).split(/\s+/, 1)[0]?.toLowerCase()
  if (!command) return null

  if (SETTINGS_COMMANDS.has(command)) {
    return { kind: 'open-settings' }
  }

  const tab = TAB_COMMANDS[command]
  if (tab) {
    return { kind: 'open-desktop-tab', tab }
  }

  return null
}

export function persistDesktopTab(tab: DesktopTab): void {
  if (typeof window === 'undefined') return
  window.localStorage.setItem(DESKTOP_TAB_STORAGE_KEY, tab)
}

export function loadPersistedDesktopTab(): DesktopTab {
  if (typeof window === 'undefined') return 'chat'
  const value = window.localStorage.getItem(DESKTOP_TAB_STORAGE_KEY)
  return value === 'workspace' || value === 'models' || value === 'settings'
    ? value
    : 'chat'
}
