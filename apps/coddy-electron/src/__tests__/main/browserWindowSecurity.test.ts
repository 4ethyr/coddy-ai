import { describe, expect, it } from 'vitest'
import { coddyBrowserWindowWebPreferences } from '../../main/browserWindowSecurity'

describe('browserWindowSecurity', () => {
  it('keeps the renderer isolated and sandboxed', () => {
    const preferences = coddyBrowserWindowWebPreferences('/app/preload.js')

    expect(preferences.preload).toBe('/app/preload.js')
    expect(preferences.contextIsolation).toBe(true)
    expect(preferences.nodeIntegration).toBe(false)
    expect(preferences.sandbox).toBe(true)
  })
})
