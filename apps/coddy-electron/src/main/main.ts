// main/main.ts
// Electron main process entry point.

import { app, BrowserWindow } from 'electron'
import * as path from 'path'
import { registerIpcHandlers, cleanupStreams } from './ipcBridge'
import { startCoddyRuntimeProcess, stopCoddyRuntimeProcess } from './runtimeProcess'

let mainWindow: BrowserWindow | null = null
const hasSingleInstanceLock = app.requestSingleInstanceLock()

if (!hasSingleInstanceLock) {
  app.quit()
}

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 680,
    minHeight: 400,
    frame: false,
    transparent: true,
    backgroundColor: '#00000000',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  })

  if (process.env.VITE_DEV_SERVER_URL) {
    mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL)
  } else {
    mainWindow.loadFile(path.join(__dirname, '../renderer/index.html'))
  }

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

app.whenReady().then(() => {
  if (!hasSingleInstanceLock) return

  const electronProcess = process as NodeJS.Process & {
    resourcesPath?: string
  }
  startCoddyRuntimeProcess({
    appPath: app.getAppPath(),
    env: process.env,
    resourcesPath: electronProcess.resourcesPath,
  })
  registerIpcHandlers()
  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('second-instance', () => {
  if (!mainWindow) return
  if (mainWindow.isMinimized()) mainWindow.restore()
  mainWindow.show()
  mainWindow.focus()
})

app.on('window-all-closed', () => {
  cleanupStreams()
  stopCoddyRuntimeProcess()
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('before-quit', () => {
  cleanupStreams()
  stopCoddyRuntimeProcess()
})
