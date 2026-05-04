import type { BrowserWindowConstructorOptions } from 'electron'

export function coddyBrowserWindowWebPreferences(
  preloadPath: string,
): NonNullable<BrowserWindowConstructorOptions['webPreferences']> {
  return {
    preload: preloadPath,
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: true,
  }
}
