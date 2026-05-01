import type { DesktopTab } from '@/presentation/components/Sidebar'

export type UiSlashCommand =
  | { kind: 'open-settings' }
  | { kind: 'open-desktop-tab'; tab: DesktopTab }
  | { kind: 'agent-workflow'; prompt: string }

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
const WORKFLOW_COMMANDS = new Set(['plan', 'review', 'test', 'tests'])

export function resolveUiSlashCommand(input: string): UiSlashCommand | null {
  const trimmed = input.trim()
  if (!trimmed.startsWith('/')) return null

  const [rawCommand = '', ...goalParts] = trimmed.slice(1).split(/\s+/)
  const command = rawCommand.toLowerCase()
  if (!command) return null

  if (SETTINGS_COMMANDS.has(command)) {
    return { kind: 'open-settings' }
  }

  const tab = TAB_COMMANDS[command]
  if (tab) {
    return { kind: 'open-desktop-tab', tab }
  }

  if (WORKFLOW_COMMANDS.has(command)) {
    const goal = goalParts.join(' ').trim()
    if (!goal) return null
    return {
      kind: 'agent-workflow',
      prompt: codingWorkflowPrompt(command, goal),
    }
  }

  return null
}

function codingWorkflowPrompt(command: string, goal: string): string {
  if (command === 'review') {
    return [
      'Code review workflow.',
      '',
      `Scope: ${goal}`,
      '',
      'Instructions:',
      '- Inspect the relevant diff, files or workspace context before making claims when tools are available.',
      '- Prioritize correctness bugs, regressions, security risks and missing tests.',
      '- Report findings first with concrete file/function evidence; keep summaries secondary.',
      '- Do not edit files unless the user explicitly asks for fixes.',
    ].join('\n')
  }

  if (command === 'test' || command === 'tests') {
    return [
      'Focused validation workflow.',
      '',
      `Goal: ${goal}`,
      '',
      'Instructions:',
      '- Identify the smallest useful test, lint, type-check or build command for this goal.',
      '- Inspect project scripts and relevant files before recommending commands when tools are available.',
      '- Run safe validation when permitted; otherwise explain the exact command and why it was not run.',
      '- Report pass/fail status, failure cause and next corrective step.',
    ].join('\n')
  }

  return [
    'Plan-only coding workflow.',
    '',
    `Goal: ${goal}`,
    '',
    'Instructions:',
    '- Inspect safe read-only workspace context before making code claims when tools are available.',
    '- Do not edit files or run mutating commands in this workflow.',
    '- Return objective, assumptions, relevant files to inspect, ordered steps, risks, validation plan and approval needs.',
    '- If evidence is missing, state exactly which read-only inspection is needed.',
  ].join('\n')
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
