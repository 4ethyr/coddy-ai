import { describe, expect, it } from 'vitest'
import {
  loadPersistedDesktopTab,
  persistDesktopTab,
  resolveUiSlashCommand,
} from '@/presentation/commands/slashCommands'

describe('slashCommands', () => {
  it('routes workspace and model commands to desktop tabs', () => {
    expect(resolveUiSlashCommand('/workspace')).toEqual({
      kind: 'open-desktop-tab',
      tab: 'workspace',
    })
    expect(resolveUiSlashCommand('/models')).toEqual({
      kind: 'open-desktop-tab',
      tab: 'models',
    })
  })

  it('accepts settings aliases including the common typo', () => {
    expect(resolveUiSlashCommand('/settings')).toEqual({
      kind: 'open-settings',
    })
    expect(resolveUiSlashCommand('/settins')).toEqual({
      kind: 'open-settings',
    })
  })

  it('does not intercept unknown slash commands', () => {
    expect(resolveUiSlashCommand('/unknown')).toBeNull()
    expect(resolveUiSlashCommand('normal prompt')).toBeNull()
  })

  it('persists the requested desktop tab for mode switches', () => {
    persistDesktopTab('workspace')

    expect(loadPersistedDesktopTab()).toBe('workspace')
  })
})
