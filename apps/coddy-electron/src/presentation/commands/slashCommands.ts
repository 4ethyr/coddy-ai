import type { DesktopTab } from '@/presentation/components/Sidebar'

export type UiSlashCommand =
  | { kind: 'open-settings' }
  | { kind: 'open-desktop-tab'; tab: DesktopTab }
  | { kind: 'open-history' }
  | { kind: 'new-session' }
  | { kind: 'set-speak'; enabled: boolean }
  | { kind: 'agent-workflow'; prompt: string }

export type UiSlashCommandSuggestion = {
  command: string
  title: string
  description: string
  insertText: string
  aliases?: string[]
  requiresArgument?: boolean
}

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
const WORKFLOW_COMMANDS = new Set([
  'code',
  'implement',
  'plan',
  'review',
  'test',
  'tests',
])

export const UI_SLASH_COMMAND_SUGGESTIONS: UiSlashCommandSuggestion[] = [
  {
    command: '/code',
    title: 'Implement with TDD',
    description: 'Inspect, plan, edit incrementally and validate a coding task.',
    insertText: '/code ',
    aliases: ['/implement'],
    requiresArgument: true,
  },
  {
    command: '/plan',
    title: 'Plan a coding task',
    description: 'Read-only plan with assumptions, risks and validation steps.',
    insertText: '/plan ',
    requiresArgument: true,
  },
  {
    command: '/review',
    title: 'Review code or a diff',
    description: 'Bug, regression, security and test-risk review workflow.',
    insertText: '/review ',
    requiresArgument: true,
  },
  {
    command: '/test',
    title: 'Choose focused validation',
    description: 'Find and run the smallest useful test, lint or build check.',
    insertText: '/test ',
    aliases: ['/tests'],
    requiresArgument: true,
  },
  {
    command: '/workspace',
    title: 'Open workspace',
    description: 'Select folders, inspect context, tools and evals.',
    insertText: '/workspace',
    aliases: ['/workspaces', '/files'],
  },
  {
    command: '/models',
    title: 'Open models',
    description: 'Load providers, choose models and inspect runtime readiness.',
    insertText: '/models',
    aliases: ['/model'],
  },
  {
    command: '/settings',
    title: 'Open settings',
    description: 'Adjust appearance, local model provider and thinking UI.',
    insertText: '/settings',
    aliases: ['/setting', '/settins', '/config'],
  },
  {
    command: '/history',
    title: 'Open history',
    description: 'Show persisted redacted chat history.',
    insertText: '/history',
  },
  {
    command: '/new',
    title: 'New session',
    description: 'Archive this chat and start a clean session.',
    insertText: '/new',
  },
  {
    command: '/speak',
    title: 'Voice response speech',
    description: 'Use /speak on or /speak off for spoken replies after voice input.',
    insertText: '/speak ',
    requiresArgument: true,
  },
]

export function resolveUiSlashCommand(input: string): UiSlashCommand | null {
  const trimmed = input.trim()
  if (!trimmed.startsWith('/')) return null

  const [rawCommand = '', ...goalParts] = trimmed.slice(1).split(/\s+/)
  const command = rawCommand.toLowerCase()
  if (!command) return null

  if (SETTINGS_COMMANDS.has(command)) {
    return { kind: 'open-settings' }
  }

  if (command === 'history') {
    return { kind: 'open-history' }
  }

  if (command === 'new') {
    return { kind: 'new-session' }
  }

  if (command === 'speak') {
    const value = goalParts[0]?.toLowerCase()
    if (value === 'on' || value === 'true' || value === 'yes') {
      return { kind: 'set-speak', enabled: true }
    }
    if (value === 'off' || value === 'false' || value === 'no') {
      return { kind: 'set-speak', enabled: false }
    }
    return null
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

export function listUiSlashCommandSuggestions(
  input: string,
): UiSlashCommandSuggestion[] {
  const query = input.trimStart()
  if (!query.startsWith('/')) return []
  if (/\s/.test(query)) return []

  const loweredQuery = query.toLowerCase()
  return UI_SLASH_COMMAND_SUGGESTIONS.filter((suggestion) => {
    const names = [suggestion.command, ...(suggestion.aliases ?? [])]
    const exactPrimaryCommand = suggestion.command === loweredQuery
    if (exactPrimaryCommand && !suggestion.requiresArgument) return false
    return names.some((name) => name.startsWith(loweredQuery))
  })
}

function codingWorkflowPrompt(command: string, goal: string): string {
  if (command === 'code' || command === 'implement') {
    return [
      'Implementation coding workflow.',
      '',
      `Goal: ${goal}`,
      '',
      'Instructions:',
      '- Explore first: inspect the smallest relevant files, scripts and tests before editing.',
      '- State a short plan with assumptions, risk, target files and validation criteria before changing code.',
      '- Use TDD for behavior changes: add or update a focused failing test before the implementation when practical.',
      '- Keep edits incremental, clean, and aligned with the existing architecture; avoid unrelated refactors and dependencies.',
      '- Use safe tools only; request approval for write, shell, network or destructive actions that require it.',
      '- Validate with the narrowest useful test first, then broaden to lint, type-check or build when warranted.',
      '- Final response must include changed files, validations run with pass/fail status, and remaining risks.',
    ].join('\n')
  }

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
