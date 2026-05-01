# Coddy Frontend — Implementation Plan

## Overview

**Project:** Coddy — REPL/CLI experience for VisionClip
**Framework:** Electron.js + TypeScript + TailwindCSS
**Backend:** Rust visionclip-daemon (Unix socket IPC)
**Methodology:** TDD (RED-GREEN-REFACTOR), Clean Code, Clean Architecture
**Visual System:** Glassmorphism (Aether Terminal design tokens)

---

## 1. Clean Architecture — Layer Definition

Following hexagonal/clean architecture with strict dependency rule:

```
src/
├── domain/              # Pure TypeScript — no framework imports
│   ├── types/           # Mirrors coddy-core Rust types
│   │   ├── events.ts        # ReplEvent (20 variants)
│   │   ├── session.ts       # ReplSession, SessionStatus, ReplMode
│   │   ├── commands.ts      # ReplCommand, ModelRef, ContextPolicy
│   │   ├── policy.ts        # AssessmentPolicy, AssistanceDecision
│   │   ├── voice.ts         # VoiceTurnIntent, VoiceState
│   │   ├── search.ts        # SearchResultContext, SearchProvider
│   │   └── context.ts       # ScreenUnderstandingContext, ScreenRegion
│   ├── reducers/        # Pure functions: state + event → new state
│   │   ├── sessionReducer.ts    # applyEvent() — mirrors ReplSession::apply_event()
│   │   └── policyEvaluator.ts   # evaluateAssistance() — mirrors policy.rs
│   └── contracts/       # Abstract ports that infrastructure implements
│       ├── IpcClient.ts       # send(request) → JobResult
│       ├── EventStream.ts     # subscribe(seq) → AsyncIterable<Envelope>
│       └── SystemBridge.ts    # Electron-specific OS integration
│
├── application/         # Use cases — orchestration, no UI imports
│   ├── SessionSyncUseCase.ts     # snapshot + incremental event polling
│   ├── VoiceShortcutUseCase.ts   # shortcut → listen → transcribe → intent
│   ├── ModelManagementUseCase.ts # select/pull/warmup/inspect models
│   ├── ReplCommandUseCase.ts     # send Ask/VoiceTurn/StopSpeaking
│   └── DoctorUseCase.ts          # diagnostic for shortcuts/config
│
├── infrastructure/      # Adapters implementing domain contracts
│   ├── ipc/
│   │   ├── ElectronIpcAdapter.ts     # Electron main ↔ renderer bridge
│   │   ├── UnixSocketAdapter.ts      # Direct Unix socket (for CLI mode)
│   │   └── IpcAdapterFactory.ts      # Selects transport based on env
│   ├── system/
│   │   ├── ElectronSystemBridge.ts   # Tray, global shortcuts, window mgmt
│   │   └── GnomeShortcutBridge.ts    # GNOME media keys integration
│   └── storage/
│       ├── ConfigStore.ts            # Read/write ~/.config/visionclip
│       └── EventCache.ts             # Local SQLite cache for offline events
│
├── presentation/        # React/Electron — only this layer imports React
│   ├── components/      # Shared UI atoms/molecules
│   │   ├── GlassPanel.tsx           # Configurable opacity/blur container
│   │   ├── CodeBlock.tsx            # Syntax highlighted code with copy
│   │   ├── TerminalPrompt.tsx       # Command input with blinking cursor
│   │   ├── MicButton.tsx            # Voice capture button with halo animation
│   │   ├── StatusBadge.tsx          # SessionStatus indicator (listening/thinking/etc)
│   │   ├── PolicyBanner.tsx         # Assessment policy indicator
│   │   ├── ModelSelector.tsx        # Dropdown with cloud/local status dots
│   │   ├── AgentPlanCard.tsx        # Plan steps with authorize button
│   │   ├── AuroraBackground.tsx     # Radial gradient decorative element
│   │   └── SearchResultCard.tsx     # Web search result display
│   ├── views/            # Full screen/page compositions
│   │   ├── FloatingTerminal/
│   │   │   ├── FloatingTerminalWindow.tsx
│   │   │   ├── FloatingTerminalHeader.tsx
│   │   │   ├── FloatingTerminalTranscript.tsx
│   │   │   └── FloatingTerminalInput.tsx
│   │   └── DesktopApp/
│   │       ├── ReplShell.tsx
│   │       ├── SideNav.tsx
│   │       ├── TopBar.tsx
│   │       ├── SessionTimeline.tsx
│   │       ├── AgentPlanPanel.tsx
│   │       ├── TerminalExecutionPanel.tsx
│   │       ├── WorkspaceContextPanel.tsx
│   │       ├── ModelManager.tsx
│   │       ├── SettingsModal.tsx
│   │       └── HistoryPanel.tsx
│   └── hooks/            # React hooks for use cases
│       ├── useSessionSync.ts       # Subscribes to event stream
│       ├── useVoiceShortcut.ts     # Voice interaction lifecycle
│       ├── useModelManagement.ts   # Model CRUD operations
│       └── useGlassmorphism.ts     # Opacity/blur reactive controls
│
├── main/                # Electron main process entry
│   ├── main.ts              # App lifecycle, window creation
│   ├── tray.ts              # System tray with quick actions
│   ├── shortcuts.ts         # Global shortcut registration
│   └── preload.ts           # Context bridge (secure IPC)
│
├── __tests__/            # Mirror src structure
│   ├── domain/
│   │   ├── sessionReducer.test.ts
│   │   ├── policyEvaluator.test.ts
│   │   └── types/contracts.test.ts
│   ├── application/
│   │   ├── SessionSyncUseCase.test.ts
│   │   ├── VoiceShortcutUseCase.test.ts
│   │   └── integration.test.ts
│   ├── infrastructure/
│   │   ├── ElectronIpcAdapter.test.ts
│   │   └── IpcAdapterFactory.test.ts
│   ├── presentation/
│   │   ├── components/GlassPanel.test.tsx
│   │   ├── components/CodeBlock.test.tsx
│   │   ├── views/FloatingTerminalWindow.test.tsx
│   │   └── views/ReplShell.test.tsx
│   └── e2e/
│       ├── floating-terminal.spec.ts
│       ├── voice-shortcut.spec.ts
│       └── model-management.spec.ts
```

### Dependency Rule (Strict)

```
presentation → application → domain ← infrastructure
                    ↑                        |
                    +--- contracts (ports) --+
```

- `domain/` imports NOTHING from other layers (pure TS, no React, no Electron)
- `application/` imports only `domain/` (orchestrates use cases)
- `infrastructure/` imports `domain/contracts` (implements ports)
- `presentation/` imports `application/` and `domain/types` (renders state)

---

## 2. TailwindCSS Design System

### 2.1 Configuration (`tailwind.config.ts`)

```ts
import type { Config } from 'tailwindcss'

export default {
  darkMode: 'class',
  content: ['./src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        surface: '#131313',
        'surface-dim': '#131313',
        'surface-bright': '#3a3939',
        'surface-container-lowest': '#0e0e0e',
        'surface-container-low': '#1c1b1b',
        'surface-container': '#201f1f',
        'surface-container-high': '#2a2a2a',
        'surface-container-highest': '#353534',
        'on-surface': '#e5e2e1',
        'on-surface-variant': '#b9cacb',
        outline: '#849495',
        'outline-variant': '#3b494b',
        primary: '#dbfcff',
        'on-primary': '#00363a',
        'primary-container': '#00f0ff',
        'on-primary-container': '#006970',
        'primary-fixed': '#7df4ff',
        'primary-fixed-dim': '#00dbe9',
        'on-primary-fixed': '#002022',
        'on-primary-fixed-variant': '#004f54',
        secondary: '#ebb2ff',
        'on-secondary': '#520072',
        'secondary-container': '#b600f8',
        'on-secondary-container': '#fff6fc',
        'secondary-fixed': '#f8d8ff',
        'secondary-fixed-dim': '#ebb2ff',
        'on-secondary-fixed': '#320047',
        'on-secondary-fixed-variant': '#74009f',
        tertiary: '#f8f5f5',
        'on-tertiary': '#313030',
        'tertiary-container': '#dcd9d8',
        'on-tertiary-container': '#5f5e5e',
        'tertiary-fixed': '#e5e2e1',
        'tertiary-fixed-dim': '#c8c6c5',
        'on-tertiary-fixed': '#1c1b1b',
        'on-tertiary-fixed-variant': '#474746',
        error: '#ffb4ab',
        'on-error': '#690005',
        'error-container': '#93000a',
        'on-error-container': '#ffdad6',
        'inverse-surface': '#e5e2e1',
        'inverse-on-surface': '#313030',
        'inverse-primary': '#006970',
        'surface-tint': '#00dbe9',
        'surface-variant': '#353534',
        background: '#131313',
        'on-background': '#e5e2e1',
      },
      fontFamily: {
        display: ['Space Grotesk', 'sans-serif'],
        headline: ['Space Grotesk', 'sans-serif'],
        body: ['Manrope', 'sans-serif'],
        label: ['Inter', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      fontSize: {
        'display-lg': ['48px', { lineHeight: '1.1', letterSpacing: '-0.02em', fontWeight: '700' }],
        'headline-md': ['24px', { lineHeight: '1.2', fontWeight: '500' }],
        'body-base': ['16px', { lineHeight: '1.6', fontWeight: '400' }],
        'label-sm': ['12px', { lineHeight: '1', letterSpacing: '0.05em', fontWeight: '600' }],
        'code-md': ['14px', { lineHeight: '1.5', fontWeight: '400' }],
      },
      borderRadius: {
        DEFAULT: '0.125rem',
        lg: '0.25rem',
        xl: '0.5rem',
        full: '0.75rem',
      },
      spacing: {
        xs: '4px',
        base: '8px',
        sm: '12px',
        gutter: '16px',
        margin: '24px',
        md: '24px',
        lg: '48px',
        xl: '80px',
      },
      backdropBlur: {
        glass: '20px',
        'glass-heavy': '30px',
        'glass-extreme': '40px',
      },
      boxShadow: {
        'glass': '0 20px 40px rgba(0, 0, 0, 0.4)',
        'glow-cyan': '0 0 8px rgba(0, 240, 255, 0.5)',
        'glow-violet': '0 0 15px rgba(182, 0, 248, 0.2)',
      },
      animation: {
        'pulse-cyan': 'pulse 2s ease-in-out infinite',
        'breathe': 'breathe 3s ease-in-out infinite',
        'spin-slow': 'spin 3s linear infinite',
        'fade-in': 'fadeIn 0.2s ease-out',
        'scale-in': 'scaleIn 0.15s ease-out',
      },
      keyframes: {
        pulse: {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.5' },
        },
        breathe: {
          '0%, 100%': { opacity: '1', transform: 'scale(1)' },
          '50%': { opacity: '0.6', transform: 'scale(1.05)' },
        },
        fadeIn: {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        scaleIn: {
          '0%': { opacity: '0', transform: 'scale(0.96)' },
          '100%': { opacity: '1', transform: 'scale(1)' },
        },
      },
    },
  },
  plugins: [],
} satisfies Config
```

### 2.2 CSS Utilities

```css
/* Glassmorphism layers — imported in globals.css */

@layer components {
  .glass-panel-level1 {
    @apply bg-surface/60 backdrop-blur-[20px] border border-white/5 shadow-glass;
  }
  .glass-panel-level2 {
    @apply bg-surface/80 backdrop-blur-[30px] border border-white/10 shadow-glass;
  }
  .glass-modal {
    @apply bg-surface/90 backdrop-blur-[40px] border border-primary-fixed-dim/20 shadow-glass shadow-[0_20px_40px_rgba(0,0,0,0.6)];
  }
  .aurora-gradient {
    @apply relative;
    background: radial-gradient(circle at top right, rgba(182, 0, 248, 0.15), transparent 40%),
                radial-gradient(circle at bottom left, rgba(0, 240, 255, 0.1), transparent 40%);
  }
  .code-bg {
    background: #0d0d0d;
  }
  .input-command-line {
    @apply bg-transparent border-0 border-b border-primary-fixed-dim/30 focus:border-primary-fixed-dim focus:shadow-[0_1px_0_rgba(0,240,255,0.5)] transition-all;
  }
  .btn-primary-outline {
    @apply border border-primary-fixed-dim/30 text-primary-fixed-dim hover:bg-primary-fixed-dim/10 hover:border-primary-fixed-dim hover:shadow-glow-cyan transition-all uppercase tracking-widest;
  }
  .btn-ghost {
    @apply text-white/40 hover:text-white transition-colors;
  }
}
```

### 2.3 GlassPanel Component API

```tsx
interface GlassPanelProps {
  opacity: number        // 0.35..0.95
  blur: number           // 0..40 (px)
  elevation: 0 | 1 | 2  // background depth level
  children: React.ReactNode
  className?: string
}
```

---

## 3. Domain Types — TypeScript mirrors of coddy-core

### 3.1 ReplEvent (20 variants)

```ts
// domain/types/events.ts
// Mirrors: crates/coddy-core/src/event.rs

export type ReplEvent =
  | { SessionStarted: { session_id: string } }
  | { RunStarted: { run_id: string } }
  | { ShortcutTriggered: { binding: string; source: ShortcutSource } }
  | { OverlayShown: { mode: ReplMode } }
  | { VoiceListeningStarted: Record<string, never> }
  | { VoiceTranscriptPartial: { text: string } }
  | { VoiceTranscriptFinal: { text: string } }
  | { ScreenCaptured: { source: ExtractionSource; bytes: number } }
  | { OcrCompleted: { chars: number; language_hint?: string } }
  | { IntentDetected: { intent: ReplIntent; confidence: number } }
  | { PolicyEvaluated: { policy: AssessmentPolicy; allowed: boolean } }
  | { ModelSelected: { model: ModelRef; role: ModelRole } }
  | { SearchStarted: { query: string; provider: string } }
  | { SearchContextExtracted: { provider: string; organic_results: number; ai_overview_present: boolean } }
  | { TokenDelta: { run_id: string; text: string } }
  | { MessageAppended: { message: ReplMessage } }
  | { ToolStarted: { name: string } }
  | { ToolCompleted: { name: string; status: ToolStatus } }
  | { TtsQueued: Record<string, never> }
  | { TtsStarted: Record<string, never> }
  | { TtsCompleted: Record<string, never> }
  | { RunCompleted: { run_id: string } }
  | { Error: { code: string; message: string } }

export interface ReplEventEnvelope {
  sequence: number
  session_id: string
  run_id: string | null
  captured_at_unix_ms: number
  event: ReplEvent
}
```

### 3.2 ReplSession State

```ts
// domain/types/session.ts
// Mirrors: crates/coddy-core/src/session.rs

export type ReplMode = 'FloatingTerminal' | 'DesktopApp'

export type SessionStatus =
  | 'Idle' | 'Listening' | 'Transcribing' | 'CapturingScreen'
  | 'BuildingContext' | 'Thinking' | 'Streaming' | 'Speaking'
  | 'AwaitingConfirmation' | 'Error'

export interface ReplSession {
  id: string
  mode: ReplMode
  status: SessionStatus
  policy: AssessmentPolicy
  selected_model: ModelRef
  voice: VoiceState
  screen_context: ScreenUnderstandingContext | null
  workspace_context: ContextItem[]
  messages: ReplMessage[]
  active_run: string | null
}
```

---

## 4. TDD Plan — Phased RED-GREEN-REFACTOR

### Phase 1: Domain Layer (Week 1)

**Goal:** Pure TypeScript types + reducers, fully tested, zero dependencies

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T1.1 | `sessionReducer` applies `SessionStarted` → Idle | `sessionReducer.ts` — all 15 event→state transitions | `domain/reducers/sessionReducer.ts` |
| T1.2 | `sessionReducer` transitions Idle→Listening on VoiceListeningStarted | (same file) | |
| T1.3 | `sessionReducer` transitions Thinking→Streaming on TokenDelta | (same file) | |
| T1.4 | `sessionReducer` reverts to Idle on RunCompleted | (same file) | |
| T1.5 | `policyEvaluator` blocks SolveMultipleChoice under SyntaxOnly | `policyEvaluator.ts` — decision matrix | `domain/reducers/policyEvaluator.ts` |
| T1.6 | `policyEvaluator` allows DebugCode under PermittedAi | (same file) | |
| T1.7 | `policyEvaluator` requires confirmation under UnknownAssessment | (same file) | |
| T1.8 | Type contracts roundtrip: encode/decode `ReplEventEnvelope` | `domain/types/events.ts` + serde test helper | `domain/types/` |
| T1.9 | `VoiceIntent` classification: "abra terminal" → OpenApplication | `domain/contracts/` spec — backend already handles this | (integration spec only) |

**Verification:** `npx vitest run src/__tests__/domain/`

### Phase 2: Application Layer (Week 2)

**Goal:** Use cases with mocked infrastructure ports

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T2.1 | `SessionSyncUseCase` calls snapshot, stores `lastSequence`, polls events | `SessionSyncUseCase.ts` | `application/SessionSyncUseCase.ts` |
| T2.2 | `SessionSyncUseCase` applies events through reducer, updates state | (same file) | |
| T2.3 | `SessionSyncUseCase` handles polling errors gracefully (retry 3x) | (same file) | |
| T2.4 | `VoiceShortcutUseCase` triggers listen, receives transcript, dispatches intent | `VoiceShortcutUseCase.ts` | `application/VoiceShortcutUseCase.ts` |
| T2.5 | `VoiceShortcutUseCase` handles shortcut conflict (busy → ignore/stop/cancel) | (same file) | |
| T2.6 | `ModelManagementUseCase` lists models, selects one, emits ModelSelected | `ModelManagementUseCase.ts` | `application/ModelManagementUseCase.ts` |
| T2.7 | `DoctorUseCase` detects shortcut environment, validates socket/binary | `DoctorUseCase.ts` | `application/DoctorUseCase.ts` |

**Verification:** `npx vitest run src/__tests__/application/`

### Phase 3: Infrastructure Layer (Week 2-3)

**Goal:** Real IPC adapters, tested against actual daemon or mock

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T3.1 | `ElectronIpcAdapter` sends `ReplCommand::Ask` and receives `BrowserQuery` | `ElectronIpcAdapter.ts` — main↔renderer bridge | `infrastructure/ipc/ElectronIpcAdapter.ts` |
| T3.2 | `ElectronIpcAdapter` handles `Error` response with `code`+`message` | (same file) | |
| T3.3 | `ElectronIpcAdapter` polls `ReplEvents` and parses envelopes | (same file) | |
| T3.4 | `IpcAdapterFactory` selects transport based on `process.env` | `IpcAdapterFactory.ts` | `infrastructure/ipc/IpcAdapterFactory.ts` |
| T3.5 | `ConfigStore` reads `~/.config/visionclip/config.toml` (matches Rust AppConfig) | `ConfigStore.ts` | `infrastructure/storage/ConfigStore.ts` |
| T3.6 | `EventCache` (SQLite via better-sqlite3) stores events locally | `EventCache.ts` | `infrastructure/storage/EventCache.ts` |

**Verification:** `npx vitest run src/__tests__/infrastructure/` (requires running daemon in CI)

### Phase 4: Presentation — Components (Week 3)

**Goal:** Atomic components with visual states, fully tested

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T4.1 | `GlassPanel` renders with correct CSS opacity/blur from props | `GlassPanel.tsx` | `presentation/components/` |
| T4.2 | `GlassPanel` handles `prefers-reduced-motion` (no blur animation) | (same file) | |
| T4.3 | `CodeBlock` renders Python syntax highlighting, copy button copies text | `CodeBlock.tsx` | (same directory) |
| T4.4 | `TerminalPrompt` shows blinking cursor, dispatches on Enter, disabled during thinking | `TerminalPrompt.tsx` | |
| T4.5 | `MicButton` shows cyan halo pulse during listening, stops on click | `MicButton.tsx` | |
| T4.6 | `StatusBadge` renders all 10 `SessionStatus` states with correct icons/animations | `StatusBadge.tsx` | |
| T4.7 | `PolicyBanner` shows correct policy text + colors for each AssessmentPolicy | `PolicyBanner.tsx` | |
| T4.8 | `ModelSelector` dropdown shows cloud/local models with status dots, selects model | `ModelSelector.tsx` | |
| T4.9 | `AgentPlanCard` shows steps, risk level, requires confirmation button for level 2+ | `AgentPlanCard.tsx` | |
| T4.10 | `SearchResultCard` shows organic results + AI overview with source links | `SearchResultCard.tsx` | |

**Verification:** `npx vitest run src/__tests__/presentation/components/`

### Phase 5: Presentation — Views (Week 4)

**Goal:** Full views integrating components with use cases

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T5.1 | `FloatingTerminalWindow` opens with scale-in animation, shows header with model badge | Integration of all floating terminal components | `presentation/views/FloatingTerminal/` |
| T5.2 | `FloatingTerminalWindow` transitions through all visual states (idle→listening→thinking→streaming→speaking) | Hooks + component orchestration | |
| T5.3 | `FloatingTerminalWindow` opacity/blur slider controls update in real time | useGlassmorphism hook | |
| T5.4 | `ReplShell` renders sidebar (240px), topbar (48px), and session timeline | Full desktop shell | `presentation/views/DesktopApp/` |
| T5.5 | `ReplShell` handles responsive breakpoints (sidebar collapses on tablet, single pane on mobile) | Responsive layout | |
| T5.6 | `ModelManager` shows Ollama models, pulls new model, shows vitals (CPU/VRAM) | Model management view | |
| T5.7 | `SettingsModal` shows config sections, never renders API keys in value | Settings view | |

**Verification:** `npx vitest run src/__tests__/presentation/views/`

### Phase 6: Electron Main Process (Week 4-5)

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T6.1 | Main process spawns BrowserWindow with preload, loads renderer | `main.ts` | `main/` |
| T6.2 | Tray icon shows, clicking opens floating terminal | `tray.ts` | |
| T6.3 | Global shortcut (configurable) triggers voice overlay | `shortcuts.ts` | |
| T6.4 | Preload exposes `window.coddyApi` with typed IPC methods | `preload.ts` | |

### Phase 7: E2E Tests (Week 5)

| ID | Test (RED first) | Implementation |
|---|---|---|
| T7.1 | Floating terminal: open with shortcut → type "explain this code" → see code explanation response | Playwright + Electron |
| T7.2 | Voice flow: click mic → speak → see transcript → see intent detected → see response | |
| T7.3 | Model switch: open model dropdown → select local model → verify status dot changes → verify next query uses new model | |
| T7.4 | Policy block: set SyntaxOnly policy → ask "write complete auth module" → see PolicyBanner with block message | |
| T7.5 | Responsive: resize window to 1366x768 → verify no overflow, all panels visible | |

**Verification:** `npx playwright test`

### Cross-Cutting Backlog: UX and Runtime Reliability

These tasks were added after reviewing the current FloatingTerminal, Desktop
model selector, runtime credential bridge, provider adapters and active-run
controls.

| ID | Test (RED first) | Implementation | Files |
|---|---|---|---|
| T8.1 | Select one word/line in FloatingTerminal transcript → only selected range is highlighted | Fix transcript selection boundaries and avoid parent-level full-message selection | `presentation/views/FloatingTerminal/`, `MessageBubble.tsx`, CSS |
| T8.2 | Select transcript text → contextual Copy action copies exactly the selected text | Selection toolbar/context action with clipboard API fallback | `presentation/components/`, `FloatingTerminal` |
| T8.3 | Press `Ctrl+Shift+C` with selected transcript text → clipboard receives the selected text | Global shortcut scoped to transcript selection; do not steal focus from inputs/menus/modals | `App.tsx`, `FloatingTerminal`, tests |
| T8.4 | Paste with `Ctrl+V` and `Ctrl+Shift+V` in the input → text is inserted at the caret | Input paste handling for both shortcuts, including disabled/active states | `InputBar.tsx` |
| T8.5 | Press `Esc` during Thinking/Streaming → active run is cancelled and the window remains open | Prioritize `cancelRun` over `window:close`; close only when idle | `App.tsx`, `useSession.ts`, `ipcBridge.ts` |
| T8.6 | Press `Esc` while voice capture or TTS is active → the active process stops cleanly | Route Escape to `cancelVoiceCapture`/`cancelSpeech`; emit stable cancelled state | `VoiceButton.tsx`, `useSession.ts`, main IPC |
| T8.7 | Thinking/loading UI renders one loading bar plus `Pressione (Esc) para parar.` | Unify thinking/loading states and remove multi-bar animation | `ThinkingIndicator.tsx`, `ConversationPanel.tsx`, `FloatingTerminal.tsx` |
| T8.8 | Select an absent Ollama model → Coddy runs pull/preload with visible progress and cancellation | Local model manager for `ollama list`, `ollama pull`, health check and warmup | `main/`, `application/`, `ModelSelector.tsx` |
| T8.9 | Select a Hugging Face local model → Coddy detects `hf`, downloads/caches or reports missing CLI | Local model manager for `hf` CLI without installing dependencies automatically | `main/`, `application/`, model manager tests |
| T8.10 | Select a vLLM model → Coddy reuses existing endpoint or starts `vllm serve` with logs and timeout | vLLM process manager with structured args, endpoint config, cancellation and health check | `main/`, `application/`, model manager tests |
| T8.11 | Loading Vertex Gemini models via gcloud/ADC then sending a prompt does not ask for a Gemini API key | Split Gemini Developer API and Vertex AI Gemini runtime routes | `runtimeCredentialBridge.ts`, `modelProviders.ts`, Rust model adapter |
| T8.12 | A Gemini API key model still uses `x-goog-api-key` and the Developer API endpoint | Preserve Gemini API-key route separately from Vertex AI OAuth route | `modelProviders.ts`, Rust model adapter |
| T8.13 | A Vertex Gemini model uses project, region and OAuth/ADC/gcloud token against Vertex AI `generateContent` | Add Vertex AI Gemini adapter and metadata propagation | `runtimeCredentialBridge.ts`, `crates/coddy-agent/src/model.rs` |
| T8.14 | Selecting a Live API/audio-only/preview model in chat shows a friendly unsupported-runtime message | Filter or gate models that do not support `generateContent`/`streamGenerateContent` | `modelProviders.ts`, `ModelSelector.tsx`, error mapper |
| T8.15 | Provider 404/credential/model errors render friendly message with retry/reload/settings action | Normalize errors with code, details, recoverability and safe diagnostics | `CommandSender.ts`, `useSession.ts`, error UI |
| T8.16 | Runtime and model errors never expose secrets in UI, logs or copied diagnostics | Redaction tests for provider errors, IPC errors and command stdout/stderr | main tests, Rust model tests |

---

## 5. Backend Integration Pattern

### 5.1 IPC Transport Abstraction

```ts
// domain/contracts/IpcClient.ts
export interface IpcClient {
  send(request: VisionRequest): Promise<JobResult>
  getSnapshot(): Promise<ReplSessionSnapshot>
  getEvents(afterSequence: number): Promise<{ events: ReplEventEnvelope[]; lastSequence: number }>
}
```

### 5.2 Session Sync Loop (Polling → Future WebSocket)

```ts
// application/SessionSyncUseCase.ts
export class SessionSyncUseCase {
  private lastSequence = 0

  async sync(ipc: IpcClient): Promise<ReplSession> {
    // 1. Initial snapshot
    const snapshot = await ipc.getSnapshot()
    this.lastSequence = snapshot.last_sequence

    // 2. Apply events through reducer
    let session = createInitialSession()
    session = sessionReducer(session, { SessionStarted: { session_id: snapshot.session.id } })
    // ... apply initial state from snapshot

    return session
  }

  async poll(ipc: IpcClient, currentSession: ReplSession): Promise<ReplSession> {
    const { events, lastSequence } = await ipc.getEvents(this.lastSequence)
    this.lastSequence = lastSequence

    let session = currentSession
    for (const envelope of events.sort((a, b) => a.sequence - b.sequence)) {
      session = sessionReducer(session, envelope.event)
    }

    return session
  }
}
```

### 5.3 Matching Rust Types (bincode ↔ JSON bridge)

For MVP, the Electron main process will shell out to `coddy session snapshot` and `coddy session events --after N` via child_process.spawn, parsing JSON output. Future phases will implement native Unix socket IPC in the main process using a native Node addon or the HTTP bridge.

---

## 6. Edge Cases & Error States

| Scenario | Handling |
|---|---|
| Daemon not running | `CoddyDoctor` shows socket path, suggests `systemctl --user start visionclip-daemon` |
| Socket connection refused | Retry 3x with exponential backoff (1s, 2s, 4s) |
| Invalid bincode payload | `IpcAdapter` throws `ProtocolError` with hex dump of first 32 bytes |
| Empty transcript after voice | Show "No speech detected. Try again?" with retry button |
| Shortcut conflict (busy speaking) | Per `ShortcutConflictPolicy`: stop TTS + start new listen |
| Assessment policy blocks request | Show `PolicyBanner` with reason, suggest rephrasing for allowed help |
| Model pull fails (network) | Show progress error, suggest checking connectivity |
| TTS queue overflow | Drop oldest queued item, log warning |

---

## 7. Tech Stack Decisions

| Concern | Decision | Rationale |
|---|---|---|
| Framework | Electron 33+ | Native OS integration (tray, global shortcuts, file system) |
| Build | Vite + electron-vite | Fast HMR, TypeScript native |
| Test runner | Vitest | Vite-native, fast, Jest-compatible API |
| Component tests | React Testing Library + @testing-library/react | Behavior-driven, not implementation-driven |
| E2E | Playwright + electron | First-class Electron support |
| State management | React hooks + useReducer | Simple, mirrors Rust reducer pattern, no Redux overhead |
| IPC serialization | JSON in MVP, bincode later | JSON works with `coddy` CLI stdout; bincode for direct socket |
| Database (cache) | better-sqlite3 | Matches Rust's eventual SQLite persistence plan |
| Code highlighting | Shiki (TextMate grammars) | Accurate, matches VS Code highlighting |
| Terminal emulation | xterm.js + node-pty | Real terminal for agentic execution panel |
| Fonts | Google Fonts (bundled locally) | Space Grotesk, Manrope, Inter, JetBrains Mono |

---

## 8. Definition of Done

- [ ] All domain types have unit tests covering valid/invalid states
- [ ] SessionReducer handles all 20 ReplEvent variants
- [ ] PolicyEvaluator covers 5×5 decision matrix
- [ ] Each use case has integration test with mocked IPC
- [ ] Each adapter has contract test against real daemon or recorded fixtures
- [ ] Every component has: render test, interaction test, accessibility snapshot
- [ ] GlassPanel opacity/blur controls work at 60fps
- [ ] Floating terminal opens < 150ms after shortcut (with warm daemon)
- [ ] All 6 visual states have correct animations (respecting prefers-reduced-motion)
- [ ] Focus ring visible on all interactive elements (keyboard navigation)
- [ ] E2E suite passes on both GNOME and Kali Linux hosts
- [ ] No `console.error` in production build (types, lint, dead code)
- [ ] `npx vitest run` passes 100% (no skipped tests)
- [ ] `npx playwright test` passes all 5 E2E scenarios
- [ ] `npx tsc --noEmit` reports zero errors
- [ ] `npx eslint src/` reports zero warnings
