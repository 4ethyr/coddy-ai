// presentation/hooks/useReplClient.ts
// Provides a singleton ReplIpcClient instance to all components.
// Injects the real ElectronReplIpcClient when running in Electron,
// or a fake stub for tests/storybook.

import type { ReplIpcClient } from '@/domain'
import { ElectronReplIpcClient } from '@/infrastructure/ipc'

let cachedClient: ReplIpcClient | null = null

function createClient(): ReplIpcClient {
  if (typeof window !== 'undefined' && window.replApi) {
    return new ElectronReplIpcClient()
  }
  // Fallback for testing — a stub that never resolves
  return {
    getSnapshot: () => new Promise(() => {}),
    getEventsAfter: () => new Promise(() => {}),
    getToolCatalog: () => Promise.resolve([]),
    getConversationHistory: () => Promise.resolve([]),
    getActiveWorkspace: () => Promise.resolve({ path: null }),
    selectWorkspaceFolder: () => Promise.resolve({ path: null, cancelled: true }),
    listProviderModels: (request) =>
      Promise.resolve({
        provider: request.provider,
        models: [],
        source: request.provider === 'ollama' ? 'local' : 'api',
        fetchedAtUnixMs: Date.now(),
    }),
    watchEvents: () => ({ [Symbol.asyncIterator]: () => ({ next: () => new Promise(() => {}) }) }),
    ask: () => new Promise(() => {}),
    runMultiagentEval: () =>
      Promise.resolve({
        suite: { score: 0, passed: 0, failed: 0, reports: [] },
        baselineWritten: null,
      }),
    runPromptBatteryEval: () =>
      Promise.resolve({
        promptCount: 0,
        stackCount: 0,
        knowledgeAreaCount: 0,
        passed: 0,
        failed: 0,
        score: 0,
        memberCoverage: {},
        failures: [],
      }),
    runQualityEval: () =>
      Promise.resolve({
        kind: 'coddy.qualityEval',
        version: 1,
        status: 'failed',
        passed: false,
        score: 0,
        checks: [],
        multiagent: { score: 0, passed: 0, failed: 0, reports: [] },
        promptBattery: {
          promptCount: 0,
          stackCount: 0,
          knowledgeAreaCount: 0,
          passed: 0,
          failed: 0,
          score: 0,
          memberCoverage: {},
          failures: [],
        },
      }),
    voiceTurn: () => new Promise(() => {}),
    stopActiveRun: () => Promise.resolve(),
    newSession: () => Promise.resolve({}),
    openConversation: () => Promise.resolve({}),
    stopSpeaking: () => Promise.resolve(),
    selectModel: () => Promise.resolve({}),
    openUi: () => Promise.resolve({}),
    captureAndExplain: () => Promise.resolve({}),
    dismissConfirmation: () => Promise.resolve({}),
    replyPermission: () => Promise.resolve({}),
    captureVoice: () => Promise.resolve({}),
    cancelVoiceCapture: () => Promise.resolve(),
  }
}

export function useReplClient(): ReplIpcClient {
  // Cache in module scope for singleton access
  if (!cachedClient) {
    cachedClient = createClient()
  }
  return cachedClient
}
