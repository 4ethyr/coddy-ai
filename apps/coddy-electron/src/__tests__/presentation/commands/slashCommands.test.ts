import { describe, expect, it } from 'vitest'
import {
  listUiSlashCommandSuggestions,
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
    expect(resolveUiSlashCommand('/tools')).toEqual({
      kind: 'open-desktop-tab',
      tab: 'workspace',
    })
    expect(resolveUiSlashCommand('/subagents')).toEqual({
      kind: 'open-desktop-tab',
      tab: 'workspace',
    })
    expect(resolveUiSlashCommand('/mcp')).toEqual({
      kind: 'open-desktop-tab',
      tab: 'workspace',
    })
    expect(resolveUiSlashCommand('/quality')).toEqual({
      kind: 'open-desktop-tab',
      tab: 'workspace',
    })
    expect(resolveUiSlashCommand('/evals')).toEqual({
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

  it('routes history and new session commands to session actions', () => {
    expect(resolveUiSlashCommand('/help')).toEqual({
      kind: 'show-help',
    })
    expect(resolveUiSlashCommand('/?')).toEqual({
      kind: 'show-help',
    })
    expect(resolveUiSlashCommand('/history')).toEqual({
      kind: 'open-history',
    })
    expect(resolveUiSlashCommand('/new')).toEqual({
      kind: 'new-session',
    })
    expect(resolveUiSlashCommand('/status')).toEqual({
      kind: 'show-status',
    })
  })

  it('routes speak commands to voice response settings', () => {
    expect(resolveUiSlashCommand('/speak on')).toEqual({
      kind: 'set-speak',
      enabled: true,
    })
    expect(resolveUiSlashCommand('/speak off')).toEqual({
      kind: 'set-speak',
      enabled: false,
    })
    expect(resolveUiSlashCommand('/speak')).toBeNull()
  })

  it('expands coding workflow commands into guarded agent prompts', () => {
    expect(resolveUiSlashCommand('/code add workspace picker')).toMatchObject({
      kind: 'agent-workflow',
      prompt: expect.stringContaining('Implementation coding workflow'),
    })
    expect(resolveUiSlashCommand('/implement retry flow')).toMatchObject({
      kind: 'agent-workflow',
      prompt: expect.stringContaining('Use TDD'),
    })
    expect(resolveUiSlashCommand('/plan add workspace picker')).toMatchObject({
      kind: 'agent-workflow',
      prompt: expect.stringContaining('Plan-only coding workflow'),
    })
    expect(resolveUiSlashCommand('/review recent diff')).toMatchObject({
      kind: 'agent-workflow',
      prompt: expect.stringContaining('Report findings first'),
    })
    expect(resolveUiSlashCommand('/tests model routing')).toMatchObject({
      kind: 'agent-workflow',
      prompt: expect.stringContaining('Focused validation workflow'),
    })
  })

  it('does not intercept unknown slash commands', () => {
    expect(resolveUiSlashCommand('/unknown')).toBeNull()
    expect(resolveUiSlashCommand('/code')).toBeNull()
    expect(resolveUiSlashCommand('/plan')).toBeNull()
    expect(resolveUiSlashCommand('normal prompt')).toBeNull()
  })

  it('lists slash command suggestions for command discovery', () => {
    expect(listUiSlashCommandSuggestions('/cod')).toMatchObject([
      { command: '/code', insertText: '/code ', requiresArgument: true },
    ])
    expect(listUiSlashCommandSuggestions('/pl')).toMatchObject([
      { command: '/plan', insertText: '/plan ', requiresArgument: true },
    ])
    expect(listUiSlashCommandSuggestions('/settins')).toMatchObject([
      { command: '/settings' },
    ])
    expect(listUiSlashCommandSuggestions('/hist')).toMatchObject([
      { command: '/history' },
    ])
    expect(listUiSlashCommandSuggestions('/he')).toMatchObject([
      { command: '/help' },
    ])
    expect(listUiSlashCommandSuggestions('/stat')).toMatchObject([
      { command: '/status' },
    ])
    expect(listUiSlashCommandSuggestions('/too')).toMatchObject([
      { command: '/tools' },
    ])
    expect(listUiSlashCommandSuggestions('/sub')).toMatchObject([
      { command: '/subagents' },
    ])
    expect(listUiSlashCommandSuggestions('/mc')).toMatchObject([
      { command: '/mcp' },
    ])
    expect(listUiSlashCommandSuggestions('/qual')).toMatchObject([
      { command: '/quality' },
    ])
    expect(listUiSlashCommandSuggestions('/eva')).toMatchObject([
      { command: '/quality' },
    ])
    expect(listUiSlashCommandSuggestions('/workspace')).toEqual([])
    expect(listUiSlashCommandSuggestions('/plan add tests')).toEqual([])
  })

  it('persists the requested desktop tab for mode switches', () => {
    persistDesktopTab('workspace')

    expect(loadPersistedDesktopTab()).toBe('workspace')
  })
})
